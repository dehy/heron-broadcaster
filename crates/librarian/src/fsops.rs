// SPDX-License-Identifier: AGPL-3.0-or-later

//! File operations that never overwrite a file.
//!
//! - [`rename_no_replace`]: an atomic rename that fails if the target
//!   exists (`renameat2(RENAME_NOREPLACE)` on Linux, `renamex_np(RENAME_EXCL)`
//!   on macOS), or a hard link followed by removing the original on file
//!   systems that support neither.
//! - [`copy_verified`]: a copy into a new file, flushed to disk and read
//!   back to check it matches, for moves across file systems.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;

use rustix::fs::{CWD, RenameFlags};
use rustix::io::Errno;

const BUFFER: usize = 256 * 1024;

/// Renames `from` to `to`, failing with [`ErrorKind::AlreadyExists`] if
/// `to` exists. Fails with [`ErrorKind::CrossesDevices`] across file systems,
/// and [`ErrorKind::Unsupported`] where neither renames without replacing
/// nor hard links are available: copy instead.
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    match rustix::fs::renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
        Ok(()) => Ok(()),
        Err(errno) if errno == Errno::XDEV => Err(ErrorKind::CrossesDevices.into()),
        Err(errno) if is_unsupported(errno) => link_then_unlink(from, to),
        Err(errno) => Err(errno.into()),
    }
}

fn is_unsupported(errno: Errno) -> bool {
    errno == Errno::INVAL || errno == Errno::NOSYS || errno == Errno::OPNOTSUPP
}

/// The fallback: `link` fails if the target exists, like a no-replace
/// rename. Interrupted between the two steps, both names point at the same
/// file, which recovery recognizes ([`same_file`]).
fn link_then_unlink(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::hard_link(from, to) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(error),
        Err(error) if error.kind() == ErrorKind::CrossesDevices => return Err(error),
        Err(_) => return Err(ErrorKind::Unsupported.into()),
    }
    std::fs::remove_file(from)
}

/// Whether two paths name the same file (hard links).
pub fn same_file(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let (a, b) = (std::fs::metadata(a)?, std::fs::metadata(b)?);
    Ok(a.dev() == b.dev() && a.ino() == b.ino())
}

/// Copies `from` into the new file `to` (which must not exist), flushes it
/// to disk, reads it back and checks it matches. Returns the BLAKE3 hash of
/// the content. On a mismatch the copy is removed and an error returned.
pub fn copy_verified(from: &Path, to: &Path) -> std::io::Result<[u8; 32]> {
    let mut source = File::open(from)?;
    // Fails if `to` exists: only a file created here is ever removed below.
    let target = OpenOptions::new().write(true).create_new(true).open(to)?;
    let result = copy_and_hash(&mut source, target).and_then(|written| {
        if full_hash(to)? == written {
            Ok(written)
        } else {
            Err(std::io::Error::other(format!(
                "the copy of {} does not match the original",
                from.display()
            )))
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_file(to);
    }
    result
}

fn copy_and_hash(source: &mut File, mut target: File) -> std::io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; BUFFER];
    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&buffer[..read]);
        target.write_all(&buffer[..read])?;
    }
    target.sync_all()?;
    Ok(*hasher.finalize().as_bytes())
}

/// BLAKE3 of the whole file.
pub fn full_hash(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(*hasher.finalize().as_bytes())
}

/// Flushes a file's content to disk.
pub fn sync_file(path: &Path) -> std::io::Result<()> {
    OpenOptions::new().write(true).open(path)?.sync_all()
}

/// Flushes a directory entry change (a rename or a new file) to disk. Not
/// every file system supports it; failures are ignored.
pub fn sync_dir(dir: &Path) {
    if let Ok(dir) = File::open(dir) {
        let _ = dir.sync_all();
    }
}

/// Removes `dir` and its parents up to (not including) `root`, as long as
/// they are empty. Directories are never removed if anything is left in them.
pub fn remove_empty_dirs(dir: &Path, root: &Path) {
    let mut current = dir;
    while current != root && current.starts_with(root) {
        if std::fs::remove_dir(current).is_err() {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_never_replace() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = (
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("c"),
        );
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let error = rename_no_replace(&a, &b).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&b).unwrap(), b"b");

        rename_no_replace(&a, &c).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read(&c).unwrap(), b"a");
    }

    #[test]
    fn the_hard_link_fallback_never_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = (
            dir.path().join("a"),
            dir.path().join("b"),
            dir.path().join("c"),
        );
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        assert_eq!(
            link_then_unlink(&a, &b).unwrap_err().kind(),
            ErrorKind::AlreadyExists
        );
        std::fs::hard_link(&a, &c).unwrap();
        assert!(same_file(&a, &c).unwrap());
        assert!(!same_file(&a, &b).unwrap());
    }

    #[test]
    fn copies_are_verified_and_never_replace() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        let content: Vec<u8> = (0..1_000_000_u32).map(|n| (n % 251) as u8).collect();
        std::fs::write(&a, &content).unwrap();
        let hash = copy_verified(&a, &b).unwrap();
        assert_eq!(hash, full_hash(&a).unwrap());
        assert_eq!(std::fs::read(&b).unwrap(), content);

        let error = copy_verified(&a, &b).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&b).unwrap(), content, "not removed either");
    }

    #[test]
    fn only_empty_directories_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::write(root.join("a/keep.jpg"), b"x").unwrap();
        remove_empty_dirs(&root.join("a/b/c"), root);
        assert!(!root.join("a/b").exists());
        assert!(root.join("a/keep.jpg").exists());
        assert!(root.exists());
    }
}
