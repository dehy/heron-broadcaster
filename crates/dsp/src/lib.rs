// SPDX-License-Identifier: AGPL-3.0-or-later

//! Audio processing: resampling, and later EQ, compressor, expander, limiter,
//! crossovers and the AGC chain (M8, M9).

pub mod resample;

pub use resample::StreamResampler;
