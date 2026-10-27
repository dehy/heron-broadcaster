// SPDX-License-Identifier: AGPL-3.0-or-later

//! Audio processing: resampling, the deck EQ (M9), and later compressor,
//! expander, limiter, crossovers and the AGC chain (M10).

pub mod eq;
pub mod resample;

pub use eq::GraphicEq;
pub use resample::StreamResampler;
