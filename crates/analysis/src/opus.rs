// SPDX-License-Identifier: AGPL-3.0-or-later

//! An Opus decoder for Symphonia, backed by libopus.
//!
//! Symphonia reads Ogg Opus (the stream, its pre-skip and its end trim)
//! but has no Opus decoder: [`OpusDecoder`] fills that gap, registered with
//! Symphonia's own decoders in [`codecs`], so the analysis and the engine
//! decode Opus files like any other. Mono and stereo only: the multistream
//! layouts beyond two channels are reported as unsupported.

use std::sync::{LazyLock, Mutex};

use symphonia::core::audio::{
    AsGenericAudioBufferRef, Audio, AudioBuffer, AudioMut, AudioSpec, Channels,
    GenericAudioBufferRef, Position,
};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
};
use symphonia::core::codecs::registry::{
    CodecRegistry, RegisterableAudioDecoder, SupportedAudioCodec,
};
use symphonia::core::errors::{Error, Result, decode_error, unsupported_error};
use symphonia::core::packet::PacketRef;

/// Opus always decodes at 48 kHz.
const SAMPLE_RATE: u32 = 48_000;
/// The longest Opus packet: 120 ms.
const MAX_FRAMES: usize = 5_760;

/// Symphonia's decoders and the Opus one.
pub fn codecs() -> &'static CodecRegistry {
    static CODECS: LazyLock<CodecRegistry> = LazyLock::new(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder::<OpusDecoder>();
        registry
    });
    &CODECS
}

/// What the decoder needs from the stream's identification header.
#[derive(Debug, PartialEq, Eq)]
struct OpusHead {
    channels: u8,
    /// Frames to drop at the start: the encoder's delay.
    pre_skip: u16,
    /// Q7.8 dB, to apply to the output.
    output_gain: i16,
}

impl OpusHead {
    /// Reads an `OpusHead` packet (RFC 7845, 5.1). Layouts other than one
    /// mono or stereo stream are unsupported.
    fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 19 || &data[..8] != b"OpusHead" {
            return decode_error("opus: invalid identification header");
        }
        // A major version other than 0 is not compatible.
        if data[8] >> 4 != 0 {
            return unsupported_error("opus: unknown header version");
        }
        let channels = data[9];
        let pre_skip = u16::from_le_bytes([data[10], data[11]]);
        let output_gain = i16::from_le_bytes([data[16], data[17]]);
        let family = data[18];
        if channels == 0 {
            return decode_error("opus: no channels");
        }
        if channels > 2 {
            return unsupported_error("opus: more than two channels");
        }
        match family {
            0 => {}
            // One stream, coupled when stereo, channels in order: the same
            // layout as family 0.
            1 => {
                let table = data.get(19..21 + usize::from(channels));
                let expected: &[u8] = if channels == 1 {
                    &[1, 0, 0]
                } else {
                    &[1, 1, 0, 1]
                };
                if table != Some(expected) {
                    return unsupported_error("opus: multistream layout");
                }
            }
            _ => return unsupported_error("opus: channel mapping family"),
        }
        Ok(Self {
            channels,
            pre_skip,
            output_gain,
        })
    }
}

/// Decodes Opus packets with libopus.
pub struct OpusDecoder {
    params: AudioCodecParameters,
    /// libopus's decoder can be sent but not shared: decoding takes
    /// `&mut self`, so the lock is never contended.
    decoder: Mutex<opus::Decoder>,
    gapless: bool,
    /// Pre-skip frames still to drop. Symphonia's Ogg reader trims the end
    /// of the stream from its last granule position, but leaves the
    /// pre-skip to the decoder.
    skip: usize,
    /// Interleaved output of the last packet.
    interleaved: Vec<f32>,
    buf: AudioBuffer<f32>,
}

impl OpusDecoder {
    pub fn try_new(params: &AudioCodecParameters, options: &AudioDecoderOptions) -> Result<Self> {
        if params.codec != CODEC_ID_OPUS {
            return unsupported_error("opus: invalid codec");
        }
        let Some(extra_data) = params.extra_data.as_deref() else {
            return unsupported_error("opus: missing identification header");
        };
        let head = OpusHead::parse(extra_data)?;
        let (layout, positions) = if head.channels == 1 {
            (opus::Channels::Mono, Position::FRONT_CENTER)
        } else {
            (
                opus::Channels::Stereo,
                Position::FRONT_LEFT | Position::FRONT_RIGHT,
            )
        };
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, layout).map_err(libopus)?;
        if head.output_gain != 0 {
            decoder
                .set_gain(i32::from(head.output_gain))
                .map_err(libopus)?;
        }
        let spec = AudioSpec::new(SAMPLE_RATE, Channels::Positioned(positions));
        Ok(Self {
            params: params.clone(),
            decoder: Mutex::new(decoder),
            gapless: options.gapless,
            skip: if options.gapless {
                usize::from(head.pre_skip)
            } else {
                0
            },
            interleaved: vec![0.0; MAX_FRAMES * usize::from(head.channels)],
            buf: AudioBuffer::new(spec, MAX_FRAMES),
        })
    }

    fn decode_packet(&mut self, packet: &PacketRef<'_>) -> Result<()> {
        self.buf.clear();
        let decoder = self
            .decoder
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let frames = decoder
            .decode_float(packet.data, &mut self.interleaved, false)
            .map_err(|_| Error::DecodeError("opus: invalid packet"))?;
        let channels = self.buf.spec().channels().count();
        self.buf.resize_uninit(frames);
        for channel in 0..channels {
            let Some(plane) = self.buf.plane_mut(channel) else {
                return decode_error("opus: missing audio plane");
            };
            for (frame, sample) in plane.iter_mut().enumerate() {
                *sample = self.interleaved[frame * channels + channel];
            }
        }
        if self.gapless {
            let skipped = self.skip.min(frames);
            self.skip -= skipped;
            self.buf.trim(
                packet.trim_start.get() as usize + skipped,
                packet.trim_end.get() as usize,
            );
        }
        Ok(())
    }
}

fn libopus(error: opus::Error) -> Error {
    tracing::debug!(%error, "libopus");
    Error::DecodeError("opus: libopus refused the stream")
}

impl AudioDecoder for OpusDecoder {
    /// After a seek: the pre-skip only applies to the start of the stream.
    fn reset(&mut self) {
        self.skip = 0;
        let decoder = self
            .decoder
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Err(error) = decoder.reset_state() {
            tracing::debug!(%error, "cannot reset the opus decoder");
        }
    }

    fn codec_info(&self) -> &CodecInfo {
        &SUPPORTED[0].info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        if let Err(error) = self.decode_packet(packet) {
            self.buf.clear();
            return Err(error);
        }
        Ok(self.buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

static SUPPORTED: [SupportedAudioCodec; 1] = [SupportedAudioCodec {
    id: CODEC_ID_OPUS,
    info: CodecInfo {
        short_name: "opus",
        long_name: "Opus (libopus)",
        profiles: &[],
    },
}];

impl RegisterableAudioDecoder for OpusDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        options: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        Ok(Box::new(Self::try_new(params, options)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        &SUPPORTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(channels: u8, gain: i16, family: u8, table: &[u8]) -> Vec<u8> {
        let mut head = b"OpusHead".to_vec();
        head.push(1);
        head.push(channels);
        head.extend(312_u16.to_le_bytes());
        head.extend(44_100_u32.to_le_bytes());
        head.extend(gain.to_le_bytes());
        head.push(family);
        head.extend_from_slice(table);
        head
    }

    #[test]
    fn reads_mono_and_stereo_heads() {
        assert_eq!(
            OpusHead::parse(&head(2, -256, 0, &[])).unwrap(),
            OpusHead {
                channels: 2,
                pre_skip: 312,
                output_gain: -256
            }
        );
        assert_eq!(OpusHead::parse(&head(1, 0, 0, &[])).unwrap().channels, 1);
        // Family 1 with one stream in order is the same layout.
        assert_eq!(
            OpusHead::parse(&head(2, 0, 1, &[1, 1, 0, 1]))
                .unwrap()
                .channels,
            2
        );
    }

    #[test]
    fn other_layouts_are_unsupported_not_broken() {
        let unsupported =
            |data: Vec<u8>| matches!(OpusHead::parse(&data), Err(Error::Unsupported(_)));
        assert!(unsupported(head(6, 0, 1, &[4, 2, 0, 4, 1, 2, 3, 5])));
        assert!(unsupported(head(3, 0, 1, &[2, 1, 0, 2, 1])));
        // Two uncoupled streams, or the channels swapped.
        assert!(unsupported(head(2, 0, 1, &[2, 0, 0, 1])));
        assert!(unsupported(head(2, 0, 1, &[1, 1, 1, 0])));
        assert!(unsupported(head(2, 0, 255, &[2, 0, 0, 1])));
        assert!(matches!(
            OpusHead::parse(b"OpusTags but not a head"),
            Err(Error::DecodeError(_))
        ));
    }
}
