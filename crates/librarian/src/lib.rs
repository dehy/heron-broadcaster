// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-librarian`: library management. Files dropped into the incoming
//! folder are identified, tagged and filed into the library; an existing
//! library is reorganized on request; every change is journaled and can be
//! undone. See `docs/library.md`.
//!
//! It is the only service with write access to the library and the
//! incoming folder.

pub mod fsops;
pub mod identify;
pub mod import;
pub mod journal;
pub mod organize;
pub mod relations;
pub mod review;
pub mod tasks;

/// Where files go in the library, shared with `heron-api`, which checks the settings.
pub use heron_library::layout;
