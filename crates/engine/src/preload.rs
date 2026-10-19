// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reads the files of the next queued tracks into memory ahead of time.
//!
//! A track is decoded only 10 s ahead of the deck. Without this, a library
//! that becomes slow or unreachable (a network share) stops the track on
//! air within seconds. Each file is read whole by a thread of its own, long
//! before it is cued; the track then decodes from memory, and plays to the
//! end even if its file is deleted or the library disappears.
//!
//! A read stuck on an unreachable share cannot be interrupted: its thread is
//! left behind. Reads in flight are therefore limited, counted by the
//! threads themselves, so a long outage does not pile threads up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::decoder::SourceFile;

/// Larger files are read from disk as they play: 512 MiB is hours of FLAC.
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Default)]
struct Shared {
    contents: Mutex<Option<Arc<[u8]>>>,
    cancelled: AtomicBool,
}

struct Preload {
    path: PathBuf,
    shared: Arc<Shared>,
}

impl Drop for Preload {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Relaxed);
    }
}

pub struct Preloader {
    max_tracks: usize,
    preloads: HashMap<u64, Preload>,
    /// Reading threads still running, forgotten ones included.
    in_flight: Arc<AtomicUsize>,
}

impl Preloader {
    /// Keeps the files of at most `max_tracks` queue entries in memory.
    pub fn new(max_tracks: usize) -> Self {
        Self {
            max_tracks,
            preloads: HashMap::new(),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Reading threads at most: twice the tracks kept, so the queue can
    /// move on while a few reads finish.
    fn max_in_flight(&self) -> usize {
        self.max_tracks.max(1) * 2
    }

    /// Reads the files of the first entries of `upcoming` (queue entry ID
    /// and file), nearest first, and forgets the others. A file not started
    /// for lack of a free reader is started on a later call.
    pub fn keep(&mut self, upcoming: impl IntoIterator<Item = (u64, PathBuf)>) {
        let wanted: Vec<(u64, PathBuf)> = upcoming.into_iter().take(self.max_tracks).collect();
        self.preloads.retain(|entry_id, preload| {
            wanted
                .iter()
                .any(|(id, path)| id == entry_id && *path == preload.path)
        });
        for (entry_id, path) in wanted {
            if self.preloads.contains_key(&entry_id) {
                continue;
            }
            if self.in_flight.load(Ordering::Acquire) >= self.max_in_flight() {
                return;
            }
            let preload = start(path, Arc::clone(&self.in_flight));
            self.preloads.insert(entry_id, preload);
        }
    }

    /// Reading threads still running.
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Acquire)
    }

    /// The file of `entry_id`, with its content when it is in memory.
    pub fn take(&mut self, entry_id: u64, path: PathBuf) -> SourceFile {
        let contents = self
            .preloads
            .remove(&entry_id)
            .filter(|preload| preload.path == path)
            .and_then(|preload| {
                preload
                    .shared
                    .contents
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
            });
        SourceFile { path, contents }
    }

    /// Files read into memory so far.
    pub fn loaded(&self) -> usize {
        self.preloads
            .values()
            .filter(|preload| {
                preload
                    .shared
                    .contents
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some()
            })
            .count()
    }
}

/// One reading thread, counted until it exits.
struct Reader(Arc<AtomicUsize>);

impl Drop for Reader {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Reads `path` on a thread of its own: it may block for a long time on an
/// unreachable network share without holding anything up.
fn start(path: PathBuf, in_flight: Arc<AtomicUsize>) -> Preload {
    let shared = Arc::new(Shared::default());
    let thread_shared = Arc::clone(&shared);
    let thread_path = path.clone();
    in_flight.fetch_add(1, Ordering::AcqRel);
    // Released when the thread exits, or here if it never starts.
    let reader = Reader(in_flight);
    let spawned = std::thread::Builder::new()
        .name("heron-preload".to_owned())
        .spawn(move || {
            let _reader = reader;
            let contents = match read(&thread_path) {
                Ok(Some(contents)) => contents,
                Ok(None) => return,
                Err(error) => {
                    // The deck reads it from disk, and skips it if it is gone.
                    tracing::warn!(path = %thread_path.display(), %error, "cannot preload a queued track");
                    metrics::counter!("heron_engine_preload_failures_total").increment(1);
                    return;
                }
            };
            if !thread_shared.cancelled.load(Ordering::Relaxed) {
                *thread_shared
                    .contents
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(contents);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot start a preload thread");
    }
    Preload { path, shared }
}

/// The whole file; `None` when it is too large to keep in memory.
fn read(path: &std::path::Path) -> std::io::Result<Option<Arc<[u8]>>> {
    if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
        return Ok(None);
    }
    Ok(Some(std::fs::read(path)?.into()))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn wait_loaded(preloader: &Preloader, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while preloader.loaded() < count {
            assert!(Instant::now() < deadline, "files never loaded");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn keeps_the_next_files_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..3)
            .map(|n| {
                let path = dir.path().join(format!("{n}.mp3"));
                std::fs::write(&path, format!("track {n}")).unwrap();
                path
            })
            .collect();
        let mut preloader = Preloader::new(2);
        preloader.keep((1..).zip(paths.iter().cloned()));
        wait_loaded(&preloader, 2);
        assert_eq!(preloader.loaded(), 2);

        // Deleted after preloading: still there.
        std::fs::remove_file(&paths[0]).unwrap();
        let file = preloader.take(1, paths[0].clone());
        assert_eq!(file.contents.as_deref(), Some(&b"track 0"[..]));

        // Not preloaded (third): read from disk.
        let file = preloader.take(3, paths[2].clone());
        assert_eq!(file.contents, None);

        // A queue change forgets entries no longer upcoming.
        preloader.keep([(3, paths[2].clone())]);
        wait_loaded(&preloader, 1);
        assert_eq!(preloader.take(2, paths[1].clone()).contents, None);
    }

    /// Named pipes block their readers, like files on an unreachable share.
    #[cfg(unix)]
    #[test]
    fn stuck_reads_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let pipes: Vec<PathBuf> = (0..8)
            .map(|n| {
                let path = dir.path().join(format!("{n}.mp3"));
                let made = std::process::Command::new("mkfifo")
                    .arg(&path)
                    .status()
                    .unwrap();
                assert!(made.success());
                path
            })
            .collect();
        let mut preloader = Preloader::new(2);
        // The queue moves on four times while every read is stuck.
        for start in [0, 2, 4, 6] {
            preloader.keep((start..start + 2).map(|n| (n as u64, pipes[n].clone())));
        }
        assert_eq!(preloader.in_flight(), 4, "at most twice the tracks kept");

        // The share comes back: the stuck readers finish and free their slots.
        for path in &pipes[..4] {
            drop(std::fs::OpenOptions::new().write(true).open(path).unwrap());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while preloader.in_flight() > 0 {
            assert!(Instant::now() < deadline, "readers never exited");
            std::thread::sleep(Duration::from_millis(2));
        }
        preloader.keep([(6, pipes[6].clone()), (7, pipes[7].clone())]);
        assert_eq!(preloader.in_flight(), 2);
        for path in &pipes[6..] {
            drop(std::fs::OpenOptions::new().write(true).open(path).unwrap());
        }
    }

    #[test]
    fn a_new_file_for_an_entry_is_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.mp3");
        let new = dir.path().join("new.mp3");
        std::fs::write(&old, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        let mut preloader = Preloader::new(2);
        preloader.keep([(1, old.clone())]);
        wait_loaded(&preloader, 1);
        preloader.keep([(1, new.clone())]);
        wait_loaded(&preloader, 1);
        assert_eq!(preloader.take(1, old).contents, None);
        preloader.keep([(1, new.clone())]);
        wait_loaded(&preloader, 1);
        assert_eq!(
            preloader.take(1, new).contents.as_deref(),
            Some(&b"new"[..])
        );
    }
}
