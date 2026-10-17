// SPDX-License-Identifier: AGPL-3.0-or-later

//! Chromaprint fingerprints, looked up on AcoustID to identify tracks.
//!
//! Like `fpcalc`, only the first two minutes are fingerprinted: that is what
//! AcoustID indexes. The analyzer fingerprints during the analysis decode,
//! and the librarian fingerprints incoming files before filing them; both
//! use [`Fingerprinter`], so they get the same fingerprint.

use std::path::Path;

use base64::Engine;
use rusty_chromaprint::{Configuration, FingerprintCompressor};

use crate::{Decoder, Error};

/// Seconds of audio fingerprinted from the start of a track.
pub const FINGERPRINT_SECONDS: u32 = 120;

/// A compressed Chromaprint fingerprint (algorithm 2, the `fpcalc` default).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chromaprint(Vec<u8>);

impl Chromaprint {
    /// Wraps a fingerprint read back from the database.
    pub fn from_compressed(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The form AcoustID expects: URL-safe base64 without padding, as
    /// printed by `fpcalc`.
    pub fn encoded(&self) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&self.0)
    }
}

/// Fingerprints the first [`FINGERPRINT_SECONDS`] of the audio pushed into it.
pub struct Fingerprinter {
    config: Configuration,
    inner: rusty_chromaprint::Fingerprinter,
    /// Interleaved samples still wanted.
    remaining: usize,
    buffer: Vec<i16>,
}

impl Fingerprinter {
    pub fn new(rate: u32, channels: usize) -> Result<Self, Error> {
        let config = Configuration::preset_test2();
        let mut inner = rusty_chromaprint::Fingerprinter::new(&config);
        let channel_count = u32::try_from(channels).unwrap_or(0);
        inner
            .start(rate, channel_count)
            .map_err(|_| Error::UnsupportedFormat { rate, channels })?;
        Ok(Self {
            config,
            inner,
            remaining: FINGERPRINT_SECONDS as usize * rate as usize * channels,
            buffer: Vec::new(),
        })
    }

    /// Whether enough audio was pushed: more is ignored.
    pub fn is_full(&self) -> bool {
        self.remaining == 0
    }

    pub fn push(&mut self, interleaved: &[f32]) {
        let take = interleaved.len().min(self.remaining);
        if take == 0 {
            return;
        }
        self.remaining -= take;
        self.buffer.clear();
        self.buffer.extend(
            interleaved[..take]
                .iter()
                .map(|&sample| (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16),
        );
        self.inner.consume(&self.buffer);
    }

    /// The fingerprint, or `None` when the audio was too short to get one.
    pub fn finish(mut self) -> Option<Chromaprint> {
        self.inner.finish();
        let raw = self.inner.fingerprint();
        (!raw.is_empty())
            .then(|| Chromaprint(FingerprintCompressor::from(&self.config).compress(raw)))
    }
}

/// Fingerprints a file, decoding only the first [`FINGERPRINT_SECONDS`].
/// CPU-bound: run it on a blocking thread.
pub fn fingerprint_file(path: &Path) -> Result<Option<Chromaprint>, Error> {
    let mut decoder = Decoder::open(path)?;
    let mut fingerprinter: Option<Fingerprinter> = None;
    while let Some(block) = decoder.next_block()? {
        let fingerprinter = match &mut fingerprinter {
            Some(fingerprinter) => fingerprinter,
            None => fingerprinter.insert(Fingerprinter::new(block.rate, block.channels)?),
        };
        fingerprinter.push(block.samples);
        if fingerprinter.is_full() {
            break;
        }
    }
    Ok(fingerprinter.ok_or(Error::Empty)?.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path, seconds: f64, freqs: &[f64]) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 22_050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for n in 0..(seconds * 22_050.0) as u32 {
            let t = f64::from(n) / 22_050.0;
            // A different note every half second, so there is something to fingerprint.
            let freq = freqs[(t * 2.0) as usize % freqs.len()];
            let sample = (0.3 * (std::f64::consts::TAU * freq * t).sin() * 32_767.0) as i16;
            writer.write_sample(sample).unwrap();
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn same_audio_same_fingerprint_different_audio_different_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        let c = dir.path().join("c.wav");
        write_wav(&a, 10.0, &[440.0, 660.0, 550.0]);
        write_wav(&b, 10.0, &[440.0, 660.0, 550.0]);
        write_wav(&c, 10.0, &[300.0, 900.0, 1200.0, 700.0]);

        let fa = fingerprint_file(&a).unwrap().unwrap();
        assert_eq!(fa, fingerprint_file(&b).unwrap().unwrap());
        assert_ne!(fa, fingerprint_file(&c).unwrap().unwrap());
        // Algorithm 2 in the compressed header.
        assert_eq!(fa.as_bytes()[0], 1);
        assert!(!fa.encoded().contains(['+', '/', '=']));
    }

    #[test]
    fn only_the_first_two_minutes_count() {
        let dir = tempfile::tempdir().unwrap();
        let short = dir.path().join("short.wav");
        let long = dir.path().join("long.wav");
        write_wav(&short, f64::from(FINGERPRINT_SECONDS), &[440.0, 660.0]);
        write_wav(
            &long,
            f64::from(FINGERPRINT_SECONDS) + 30.0,
            &[440.0, 660.0],
        );
        assert_eq!(
            fingerprint_file(&short).unwrap(),
            fingerprint_file(&long).unwrap()
        );
    }
}
