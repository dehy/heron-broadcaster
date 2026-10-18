// SPDX-License-Identifier: AGPL-3.0-or-later

//! What a file's audio is: codec, bitrate (constant or variable), sample
//! rate and channels, read with the tags.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitrateMode {
    Cbr,
    Vbr,
    Lossless,
}

impl BitrateMode {
    /// The value of `tracks.bitrate_mode`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cbr => "cbr",
            Self::Vbr => "vbr",
            Self::Lossless => "lossless",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioProperties {
    /// `MP3`, `AAC`, `FLAC`, … `None` when unknown.
    pub codec: Option<String>,
    /// Average audio bitrate.
    pub bitrate_kbps: Option<u32>,
    /// `None` when the format does not say (AAC).
    pub bitrate_mode: Option<BitrateMode>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}

/// The properties of a file `lofty` has read.
pub(crate) fn of(path: &Path, tagged: &TaggedFile) -> AudioProperties {
    let properties = tagged.properties();
    let (codec, mode) = match tagged.file_type() {
        FileType::Mpeg => {
            let (codec, mode) = mpeg(path).unwrap_or(("MP3", None));
            (codec, mode)
        }
        FileType::Mp4 => match mp4_codec(path) {
            Some(lofty::mp4::Mp4Codec::ALAC) => ("ALAC", Some(BitrateMode::Lossless)),
            Some(lofty::mp4::Mp4Codec::FLAC) => ("FLAC", Some(BitrateMode::Lossless)),
            Some(lofty::mp4::Mp4Codec::MP3) => ("MP3", None),
            _ => ("AAC", None),
        },
        FileType::Aac => ("AAC", None),
        FileType::Flac => ("FLAC", Some(BitrateMode::Lossless)),
        FileType::Wav => ("PCM", Some(BitrateMode::Lossless)),
        FileType::Aiff => ("PCM", Some(BitrateMode::Lossless)),
        FileType::Ape => ("APE", Some(BitrateMode::Lossless)),
        FileType::WavPack => ("WavPack", None),
        FileType::Vorbis => ("Vorbis", Some(BitrateMode::Vbr)),
        FileType::Opus => ("Opus", Some(BitrateMode::Vbr)),
        FileType::Speex => ("Speex", Some(BitrateMode::Vbr)),
        FileType::Mpc => ("Musepack", Some(BitrateMode::Vbr)),
        _ => ("", None),
    };
    AudioProperties {
        codec: (!codec.is_empty()).then(|| codec.to_owned()),
        bitrate_kbps: properties
            .audio_bitrate()
            .or_else(|| properties.overall_bitrate())
            .filter(|&kbps| kbps > 0),
        bitrate_mode: mode,
        sample_rate: properties.sample_rate().filter(|&rate| rate > 0),
        channels: properties.channels().filter(|&channels| channels > 0),
    }
}

fn mp4_codec(path: &Path) -> Option<lofty::mp4::Mp4Codec> {
    use lofty::config::ParseOptions;
    let mut file = File::open(path).ok()?;
    let mp4 =
        lofty::mp4::Mp4File::read_from(&mut file, ParseOptions::new().read_tags(false)).ok()?;
    mp4.properties().codec()
}

/// Bytes read from the first audio frame on to tell CBR from VBR.
const MPEG_SCAN: u64 = 128 * 1024;
/// Frames compared when the file has no Xing, Info or VBRI header.
const MPEG_FRAMES: usize = 200;

/// The layer and bitrate mode of an MPEG audio file: a Xing (VBR) or Info
/// (CBR) header in the first frame, a VBRI header (VBR), or else whether
/// the bitrate changes over the first frames.
fn mpeg(path: &Path) -> Option<(&'static str, Option<BitrateMode>)> {
    let mut file = File::open(path).ok()?;
    let mut header = [0_u8; 10];
    file.read_exact(&mut header).ok()?;
    let start = if &header[..3] == b"ID3" {
        let size = header[6..10]
            .iter()
            .fold(0_u64, |size, &byte| (size << 7) | u64::from(byte & 0x7f));
        let footer = if header[5] & 0x10 != 0 { 10 } else { 0 };
        10 + size + footer
    } else {
        0
    };
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(MPEG_SCAN).read_to_end(&mut bytes).ok()?;
    let mode = mpeg_mode(&bytes);
    let layer = mpeg_frame(&bytes, first_frame(&bytes)?).map(|frame| frame.layer);
    let codec = match layer {
        Some(1) => "MP1",
        Some(2) => "MP2",
        _ => "MP3",
    };
    Some((codec, mode))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MpegFrame {
    mpeg1: bool,
    layer: u8,
    mono: bool,
    bitrate_index: u8,
    len: usize,
}

const BITRATES_V1: [[u32; 15]; 3] = [
    [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
    ],
    [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
    ],
    [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ],
];
const BITRATES_V2: [[u32; 15]; 3] = [
    [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
    ],
    [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
];

/// The frame header at `at`, if there is a valid one.
fn mpeg_frame(bytes: &[u8], at: usize) -> Option<MpegFrame> {
    let header = bytes.get(at..at + 4)?;
    if header[0] != 0xff || header[1] & 0xe0 != 0xe0 {
        return None;
    }
    let version = (header[1] >> 3) & 0b11; // 3: MPEG 1, 2: MPEG 2, 0: MPEG 2.5
    let layer = match (header[1] >> 1) & 0b11 {
        3 => 1,
        2 => 2,
        1 => 3,
        _ => return None,
    };
    let bitrate_index = header[2] >> 4;
    let rate_index = (header[2] >> 2) & 0b11;
    if version == 1 || bitrate_index == 0 || bitrate_index == 15 || rate_index == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    let rate = [44_100, 48_000, 32_000][usize::from(rate_index)]
        >> match version {
            3 => 0,
            2 => 1,
            _ => 2,
        };
    let table = if mpeg1 { &BITRATES_V1 } else { &BITRATES_V2 };
    let bitrate = table[usize::from(layer - 1)][usize::from(bitrate_index)] * 1000;
    let padding = u32::from((header[2] >> 1) & 1);
    let len = match (layer, mpeg1) {
        (1, _) => (12 * bitrate / rate + padding) * 4,
        (3, false) => 72 * bitrate / rate + padding,
        _ => 144 * bitrate / rate + padding,
    };
    Some(MpegFrame {
        mpeg1,
        layer,
        mono: header[3] >> 6 == 3,
        bitrate_index,
        len: usize::try_from(len).ok().filter(|&len| len > 4)?,
    })
}

/// The first frame followed by another one where its length says.
fn first_frame(bytes: &[u8]) -> Option<usize> {
    (0..bytes.len().saturating_sub(4)).find(|&at| {
        mpeg_frame(bytes, at).is_some_and(|frame| mpeg_frame(bytes, at + frame.len).is_some())
    })
}

fn mpeg_mode(bytes: &[u8]) -> Option<BitrateMode> {
    let at = first_frame(bytes)?;
    let frame = mpeg_frame(bytes, at)?;
    if frame.layer == 3 {
        let side_info = match (frame.mpeg1, frame.mono) {
            (true, false) => 32,
            (true, true) | (false, false) => 17,
            (false, true) => 9,
        };
        match bytes.get(at + 4 + side_info..at + 8 + side_info) {
            Some(b"Xing") => return Some(BitrateMode::Vbr),
            Some(b"Info") => return Some(BitrateMode::Cbr),
            _ => {}
        }
        if bytes.get(at + 36..at + 40) == Some(b"VBRI") {
            return Some(BitrateMode::Vbr);
        }
    }
    let mut next = at;
    let mut seen = 0;
    while seen < MPEG_FRAMES {
        let Some(current) = mpeg_frame(bytes, next) else {
            break;
        };
        if current.bitrate_index != frame.bitrate_index {
            return Some(BitrateMode::Vbr);
        }
        next += current.len;
        seen += 1;
    }
    (seen > 1).then_some(BitrateMode::Cbr)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layer III frame: MPEG 1, 44.1 kHz, stereo, `bitrate_index`.
    fn frame(bitrate_index: u8) -> Vec<u8> {
        let mut frame = vec![0xff, 0xfb, bitrate_index << 4, 0x00];
        let len = mpeg_frame(&frame, 0).unwrap().len;
        frame.resize(len, 0);
        frame
    }

    #[test]
    fn frames_tell_cbr_from_vbr() {
        let cbr: Vec<u8> = (0..20).flat_map(|_| frame(9)).collect();
        assert_eq!(mpeg_mode(&cbr), Some(BitrateMode::Cbr));
        let vbr: Vec<u8> = (0..20)
            .flat_map(|n| frame(if n % 3 == 0 { 9 } else { 11 }))
            .collect();
        assert_eq!(mpeg_mode(&vbr), Some(BitrateMode::Vbr));

        let mut xing = frame(9);
        xing[36..40].copy_from_slice(b"Xing");
        let tagged: Vec<u8> = xing
            .into_iter()
            .chain((0..5).flat_map(|_| frame(9)))
            .collect();
        assert_eq!(mpeg_mode(&tagged), Some(BitrateMode::Vbr));
        let mut info = frame(9);
        info[36..40].copy_from_slice(b"Info");
        let tagged: Vec<u8> = info
            .into_iter()
            .chain((0..5).flat_map(|_| frame(11)))
            .collect();
        assert_eq!(mpeg_mode(&tagged), Some(BitrateMode::Cbr));
    }

    #[test]
    fn junk_before_the_first_frame_is_skipped() {
        let mut bytes = vec![0xff, 0x00, 0x12];
        bytes.extend((0..10).flat_map(|_| frame(9)));
        assert_eq!(first_frame(&bytes), Some(3));
        assert_eq!(mpeg_mode(&[0_u8; 100]), None);
    }
}
