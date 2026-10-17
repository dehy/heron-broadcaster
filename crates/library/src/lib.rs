// SPDX-License-Identifier: AGPL-3.0-or-later

//! Library files, shared by the scanner in `heron-api` (read-only) and
//! `heron-librarian`, the only service that moves files and writes tags.
//!
//! - [`files`]: which files are audio, and how their content is identified.
//! - [`tags`]: the tags Heron reads and writes.
//! - [`layout`]: where files go in the library (`{album_artist}/{album}/…`).
//! - [`lock`]: the lock that keeps the scanner and the librarian from
//!   working on the library at the same time.

pub mod files;
pub mod layout;
pub mod lock;
pub mod tags;

pub use files::{content_hash, is_audio, is_hidden};
pub use lock::LibraryLock;
pub use tags::{MusicBrainzIds, TagError, Tags};
