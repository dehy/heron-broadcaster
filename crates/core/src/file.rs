// SPDX-License-Identifier: AGPL-3.0-or-later

//! Library files: paths relative to the library root, and the size + mtime
//! stamp used to detect changes without reading the file.

use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// A path relative to the music library root, with `/` separators.
///
/// Stored instead of absolute paths so every service can mount the library
/// wherever it likes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LibraryPath(String);

impl LibraryPath {
    /// Builds the library path of `file` under `root`. Returns `None` when the
    /// file is outside the root or its path is not valid UTF-8.
    pub fn from_file(root: &Path, file: &Path) -> Option<Self> {
        let relative = file.strip_prefix(root).ok()?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_str()?),
                _ => return None,
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(Self(parts.join("/")))
    }

    /// Wraps a path read back from the database.
    pub fn from_stored(path: String) -> Self {
        Self(path)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Absolute path of this file under `root`.
    pub fn resolve(&self, root: &Path) -> PathBuf {
        self.0
            .split('/')
            .fold(root.to_path_buf(), |path, part| path.join(part))
    }
}

impl std::fmt::Display for LibraryPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Size and modification time of a file, stored with each track.
/// A file whose stamp is unchanged is not read again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    /// Modification time in microseconds since the Unix epoch.
    pub mtime_us: i64,
}

impl FileStamp {
    pub fn from_metadata(metadata: &Metadata) -> Self {
        let mtime_us = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| {
                i64::try_from(since.as_micros()).unwrap_or(i64::MAX)
            });
        Self {
            size: metadata.len(),
            mtime_us,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_paths_are_relative_with_forward_slashes() {
        let root = Path::new("/music");
        let path =
            LibraryPath::from_file(root, Path::new("/music/Jazz/Miles/So What.flac")).unwrap();
        assert_eq!(path.as_str(), "Jazz/Miles/So What.flac");
        assert_eq!(
            path.resolve(Path::new("/srv/library")),
            PathBuf::from("/srv/library/Jazz/Miles/So What.flac")
        );
    }

    #[test]
    fn files_outside_the_root_are_rejected() {
        let root = Path::new("/music");
        assert!(LibraryPath::from_file(root, Path::new("/other/a.mp3")).is_none());
        assert!(LibraryPath::from_file(root, Path::new("/music")).is_none());
    }
}
