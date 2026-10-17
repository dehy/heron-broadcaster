// SPDX-License-Identifier: AGPL-3.0-or-later

//! `heron-engine`: real-time playout. Decks, mixer, crossfades, pacing clock,
//! encoders and outputs. See `docs/audio-engine.md`.
//!
//! The engine never connects to the database and must keep playing when
//! every other service is down.

pub mod mixer;
