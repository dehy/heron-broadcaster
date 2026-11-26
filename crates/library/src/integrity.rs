// SPDX-License-Identifier: AGPL-3.0-or-later

//! Damage a file can play with but should not be kept with: a decoder
//! skips it, a browser or a player that seeks does not.
//!
//! MP3 only for now, read frame by frame (MPEG-1, 2 and 2.5, layer III):
//!
//! - [`Damage::SeekTable`]: the VBR header (Xing or Info) that tells where
//!   each moment of a variable bitrate file is has no frame count, or a
//!   seek table that is empty or out of order. Seeking then lands at the
//!   start, or at the wrong place.
//! - [`Damage::Frames`]: bytes that are not audio between the frames
//!   (a damaged copy), or a last frame cut short.
//! - [`Damage::Mixed`]: the sample rate or the channels change along the
//!   file: two files glued together.
//!
//! A file can have several: each is reported.

use std::path::Path;

/// What is wrong with a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// The VBR seek table is empty or wrong: seeking does not work.
    SeekTable,
    /// Not audio between frames, or the file is cut short.
    Frames {
        /// Bytes that are not part of a frame.
        bytes: u64,
        /// Whether the last frame is cut short.
        truncated: bool,
    },
    /// The sample rate or the channels change along the file.
    Mixed {
        /// Frames that differ from the first one.
        frames: u64,
    },
}

/// Checks a file. Formats not checked yet, and files that cannot be read,
/// have nothing to report: decoding finds those.
pub fn check(path: &Path) -> Vec<Damage> {
    let is_mp3 = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"));
    if !is_mp3 {
        return Vec::new();
    }
    match std::fs::read(path) {
        Ok(bytes) => check_mp3(&bytes),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "cannot read the file to check it");
            Vec::new()
        }
    }
}

/// An MPEG audio frame header (layer III).
#[derive(Clone, Copy, Debug)]
struct Frame {
    /// MPEG-1 (true), else 2 or 2.5.
    mpeg1: bool,
    mono: bool,
    sample_rate: u32,
    /// Length in bytes, the header included.
    length: usize,
}

const BITRATES_V1: [u32; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
const BITRATES_V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

fn frame_at(bytes: &[u8], at: usize) -> Option<Frame> {
    let header = bytes.get(at..at + 4)?;
    if header[0] != 0xFF || header[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (header[1] >> 3) & 3; // 0: 2.5, 2: 2, 3: 1
    let layer = (header[1] >> 1) & 3; // 1: layer III
    let bitrate = usize::from(header[2] >> 4);
    let rate = (header[2] >> 2) & 3;
    if version == 1 || layer != 1 || bitrate == 0 || bitrate == 15 || rate == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    let sample_rate = match (version, rate) {
        (3, 0) => 44_100,
        (3, 1) => 48_000,
        (3, _) => 32_000,
        (2, 0) => 22_050,
        (2, 1) => 24_000,
        (2, _) => 16_000,
        (_, 0) => 11_025,
        (_, 1) => 12_000,
        _ => 8_000,
    };
    let kbps = if mpeg1 {
        BITRATES_V1[bitrate]
    } else {
        BITRATES_V2[bitrate]
    };
    let padding = u32::from((header[2] >> 1) & 1);
    let factor = if mpeg1 { 144_000 } else { 72_000 };
    let length = usize::try_from(factor * kbps / sample_rate + padding).ok()?;
    Some(Frame {
        mpeg1,
        mono: header[3] >> 6 == 3,
        sample_rate,
        length,
    })
}

/// A frame there, followed by another one (or by the end of the audio):
/// two headers in a row are not chance.
fn sure_frame_at(bytes: &[u8], at: usize, end: usize) -> Option<Frame> {
    let frame = frame_at(bytes, at)?;
    let next = at + frame.length;
    (next >= end || frame_at(bytes, next).is_some()).then_some(frame)
}

/// Where the audio starts: after the ID3v2 tag, if any.
fn audio_start(bytes: &[u8]) -> usize {
    if bytes.len() >= 10 && &bytes[..3] == b"ID3" {
        let size = bytes[6..10]
            .iter()
            .fold(0_usize, |size, byte| (size << 7) | usize::from(byte & 0x7F));
        let footer = if bytes[5] & 0x10 == 0 { 0 } else { 10 };
        10 + size + footer
    } else {
        0
    }
}

/// Where the audio ends: before the APE and ID3v1 tags, if any.
fn audio_end(bytes: &[u8]) -> usize {
    let mut end = bytes.len();
    if end >= 128 && &bytes[end - 128..end - 125] == b"TAG" {
        end -= 128;
    }
    if end >= 32 && &bytes[end - 32..end - 24] == b"APETAGEX" {
        let size = u32::from_le_bytes([
            bytes[end - 20],
            bytes[end - 19],
            bytes[end - 18],
            bytes[end - 17],
        ]);
        let flags = u32::from_le_bytes([
            bytes[end - 12],
            bytes[end - 11],
            bytes[end - 10],
            bytes[end - 9],
        ]);
        let header = if flags & 0x8000_0000 == 0 { 0 } else { 32 };
        end = end.saturating_sub(usize::try_from(size).unwrap_or(0) + header);
    }
    end
}

fn check_mp3(bytes: &[u8]) -> Vec<Damage> {
    let end = audio_end(bytes);
    let mut at = audio_start(bytes).min(end);
    // Padding or junk before the first frame is a gap like any other.
    let mut skipped: u64 = 0;
    let mut frames: u64 = 0;
    let mut first: Option<(usize, Frame)> = None;
    let mut truncated = false;
    let mut mixed: u64 = 0;
    // Right after a frame, a header is trusted; after junk, two in a row are needed.
    let mut in_step = false;
    while at < end {
        let found = if in_step {
            frame_at(bytes, at)
        } else {
            sure_frame_at(bytes, at, end)
        };
        let Some(frame) = found else {
            at += 1;
            skipped += 1;
            in_step = false;
            continue;
        };
        in_step = true;
        if at + frame.length > end {
            truncated = true;
            break;
        }
        match first {
            None => first = Some((at, frame)),
            Some((_, first))
                if first.sample_rate != frame.sample_rate || first.mono != frame.mono =>
            {
                mixed += 1
            }
            Some(_) => {}
        }
        frames += 1;
        at += frame.length;
    }
    let Some((first_at, first_frame)) = first else {
        // No audio at all: decoding says so.
        return Vec::new();
    };
    let mut damage = Vec::new();
    if let Some(table) = vbr_header(bytes, first_at, first_frame) {
        // The header frame is silent and not counted by every encoder.
        let audio = frames.saturating_sub(1);
        let counted_wrong = table
            .frames
            .is_some_and(|counted| counted == 0 || counted.abs_diff(audio) > audio / 50 + 2);
        let toc_wrong = table.toc.is_some_and(|toc| {
            toc.iter().all(|&point| point == 0) || toc.windows(2).any(|pair| pair[0] > pair[1])
        });
        if table.vbr && (counted_wrong || toc_wrong) {
            damage.push(Damage::SeekTable);
        }
    }
    // A few stray bytes (padding to a boundary) are common and harmless.
    if skipped > 512 || truncated {
        damage.push(Damage::Frames {
            bytes: skipped,
            truncated,
        });
    }
    if mixed > 0 {
        damage.push(Damage::Mixed { frames: mixed });
    }
    damage
}

struct VbrHeader {
    /// Xing (variable bitrate), not Info (constant).
    vbr: bool,
    frames: Option<u64>,
    toc: Option<[u8; 100]>,
}

fn vbr_header(bytes: &[u8], at: usize, frame: Frame) -> Option<VbrHeader> {
    let side_info = match (frame.mpeg1, frame.mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    let start = at + 4 + side_info;
    let tag = bytes.get(start..start + 4)?;
    let vbr = match tag {
        b"Xing" => true,
        b"Info" => false,
        _ => return None,
    };
    let flags = u32::from_be_bytes(bytes.get(start + 4..start + 8)?.try_into().ok()?);
    let mut field = start + 8;
    let mut frames = None;
    if flags & 1 != 0 {
        frames = Some(u64::from(u32::from_be_bytes(
            bytes.get(field..field + 4)?.try_into().ok()?,
        )));
        field += 4;
    }
    if flags & 2 != 0 {
        field += 4;
    }
    let toc = if flags & 4 == 0 {
        None
    } else {
        Some(bytes.get(field..field + 100)?.try_into().ok()?)
    };
    Some(VbrHeader { vbr, frames, toc })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An MPEG-1 layer III stereo frame at 44.1 kHz, `kbps` from the table.
    fn frame(kbps_index: u8) -> Vec<u8> {
        let header = [0xFF, 0xFB, kbps_index << 4, 0x40];
        let length =
            usize::try_from(144_000 * BITRATES_V1[usize::from(kbps_index)] / 44_100).unwrap();
        let mut bytes = header.to_vec();
        bytes.resize(length, 0);
        bytes
    }

    /// A first frame carrying a Xing header with `frames` and `toc`.
    fn xing(frames: u32, toc: [u8; 100]) -> Vec<u8> {
        let mut bytes = frame(9);
        let start = 4 + 32;
        bytes[start..start + 4].copy_from_slice(b"Xing");
        bytes[start + 4..start + 8].copy_from_slice(&7_u32.to_be_bytes());
        bytes[start + 8..start + 12].copy_from_slice(&frames.to_be_bytes());
        bytes[start + 16..start + 116].copy_from_slice(&toc);
        bytes
    }

    fn rising() -> [u8; 100] {
        std::array::from_fn(|i| u8::try_from(i * 255 / 99).unwrap())
    }

    fn file(head: Vec<u8>, frames: usize) -> Vec<u8> {
        let mut bytes = head;
        for n in 0..frames {
            bytes.extend(frame(if n % 2 == 0 { 9 } else { 11 }));
        }
        bytes
    }

    #[test]
    fn a_sound_vbr_file_has_nothing_to_report() {
        assert!(check_mp3(&file(xing(200, rising()), 200)).is_empty());
    }

    #[test]
    fn an_empty_seek_table_or_frame_count_is_reported() {
        assert_eq!(
            check_mp3(&file(xing(0, rising()), 200)),
            [Damage::SeekTable]
        );
        assert_eq!(
            check_mp3(&file(xing(200, [0; 100]), 200)),
            [Damage::SeekTable]
        );
        let mut backwards = rising();
        backwards.swap(10, 90);
        assert_eq!(
            check_mp3(&file(xing(200, backwards), 200)),
            [Damage::SeekTable]
        );
    }

    #[test]
    fn junk_between_frames_and_a_cut_last_frame_are_reported() {
        // Between two frames: a frame cut by the junk would count too.
        let mut bytes = file(Vec::new(), 50);
        bytes.extend(std::iter::repeat_n(0x55, 2000));
        bytes.extend(file(Vec::new(), 50));
        assert_eq!(
            check_mp3(&bytes),
            [Damage::Frames {
                bytes: 2000,
                truncated: false
            }]
        );
        let mut cut = file(Vec::new(), 100);
        cut.truncate(cut.len() - 100);
        assert!(matches!(
            check_mp3(&cut)[..],
            [Damage::Frames {
                truncated: true,
                ..
            }]
        ));
    }

    #[test]
    fn two_files_glued_together_and_several_damages_at_once() {
        let mut bytes = file(xing(0, rising()), 100);
        // The same frames, mono.
        for _ in 0..30 {
            let mut mono = frame(9);
            mono[3] = 0xC0;
            bytes.extend(mono);
        }
        let cut = bytes.len() - 50;
        bytes.truncate(cut);
        let found = check_mp3(&bytes);
        assert_eq!(found[0], Damage::SeekTable);
        assert!(matches!(
            found[1],
            Damage::Frames {
                truncated: true,
                ..
            }
        ));
        assert_eq!(found[2], Damage::Mixed { frames: 29 });
    }

    #[test]
    fn tags_around_the_audio_are_not_damage() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x05hello".to_vec();
        bytes.extend(file(Vec::new(), 50));
        bytes.extend(b"TAG");
        bytes.resize(bytes.len() + 125, b' ');
        assert!(check_mp3(&bytes).is_empty());
    }
}
