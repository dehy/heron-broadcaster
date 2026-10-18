// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reads the files of the next queued tracks into memory ahead of time.
//!
//! A track is decoded only 10 s ahead of the deck. Without this, a library
//! that becomes slow or unreachable (a network share) stops the track on
//! air within seconds. Each file is read whole by a thread of its own, long
//! before it is cued; the track then decodes from memory, and plays to the
//! end even if its file is deleted or the library disappears.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
}

impl Preloader {
    /// Keeps the files of at most `max_tracks` queue entries in memory.
    pub fn new(max_tracks: usize) -> Self {
        Self {
            max_tracks,
            preloads: HashMap::new(),
        }
    }

    /// Reads the files of the first entries of `upcoming` (queue entry ID
    /// and file), and forgets the others.
    pub fn keep(&mut self, upcoming: impl IntoIterator<Item = (u64, PathBuf)>) {
        let wanted: HashMap<u64, PathBuf> = upcoming.into_iter().take(self.max_tracks).collect();
        self.preloads
            .retain(|entry_id, preload| wanted.get(entry_id) == Some(&preload.path));
        for (entry_id, path) in wanted {
            self.preloads.entry(entry_id).or_insert_with(|| start(path));
        }
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

/// Reads `path` on a thread of its own: it may block for a long time on an
/// unreachable network share without holding anything up.
fn start(path: PathBuf) -> Preload {
    let shared = Arc::new(Shared::default());
    let thread_shared = Arc::clone(&shared);
    let thread_path = path.clone();
    let spawned = std::thread::Builder::new()
        .name("heron-preload".to_owned())
        .spawn(move || {
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
