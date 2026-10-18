// SPDX-License-Identifier: AGPL-3.0-or-later

//! The queue kept on disk (`queue.json` in the state directory), so a
//! restarted engine resumes the track on air where it was and plays the
//! queue it had, even when the API is down.
//!
//! Saves are written by a thread of their own: a slow disk (an SD card)
//! never holds up the player.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::config::write_atomically;
use crate::player::SavedQueue;

const FILE_NAME: &str = "queue.json";

/// The saved queue; empty when there is none or it cannot be read.
pub fn load(dir: &Path) -> SavedQueue {
    let json = match std::fs::read(dir.join(FILE_NAME)) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return SavedQueue::default(),
        Err(error) => {
            tracing::error!(%error, "cannot read the saved queue");
            return SavedQueue::default();
        }
    };
    serde_json::from_slice(&json).unwrap_or_else(|error| {
        tracing::error!(%error, "ignoring the saved queue");
        SavedQueue::default()
    })
}

/// Writes `queue` now, on the calling thread.
pub fn save(dir: &Path, queue: &SavedQueue) -> std::io::Result<()> {
    let json = serde_json::to_vec(queue).map_err(std::io::Error::other)?;
    write_atomically(dir, FILE_NAME, &json)
}

#[derive(Default)]
struct Pending {
    latest: Option<SavedQueue>,
    stop: bool,
}

/// Saves queues in the background, only the latest when several wait.
pub struct QueueWriter {
    shared: Arc<(Mutex<Pending>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl QueueWriter {
    pub fn start(dir: PathBuf) -> std::io::Result<Self> {
        let shared = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let thread_shared = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("heron-queue-file".to_owned())
            .spawn(move || {
                let (lock, wake) = &*thread_shared;
                loop {
                    let queue = {
                        let mut pending = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        while pending.latest.is_none() && !pending.stop {
                            pending = wake
                                .wait(pending)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        match pending.latest.take() {
                            Some(queue) => queue,
                            None => return,
                        }
                    };
                    if let Err(error) = save(&dir, &queue) {
                        tracing::error!(%error, "cannot save the queue");
                    }
                }
            })?;
        Ok(Self {
            shared,
            handle: Some(handle),
        })
    }

    /// Saves `queue` soon, replacing a save not written yet.
    pub fn submit(&self, queue: SavedQueue) {
        let (lock, wake) = &*self.shared;
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .latest = Some(queue);
        wake.notify_one();
    }
}

impl Drop for QueueWriter {
    /// Writes what is pending, then stops.
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop = true;
        wake.notify_one();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_in_the_background_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), SavedQueue::default());
        let writer = QueueWriter::start(dir.path().to_path_buf()).unwrap();
        for next_entry_id in 1..=20 {
            writer.submit(SavedQueue {
                next_entry_id,
                ..SavedQueue::default()
            });
        }
        drop(writer);
        assert_eq!(load(dir.path()).next_entry_id, 20);

        std::fs::write(dir.path().join(FILE_NAME), b"{broken").unwrap();
        assert_eq!(load(dir.path()), SavedQueue::default());
    }
}
