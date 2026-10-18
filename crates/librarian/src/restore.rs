// SPDX-License-Identifier: AGPL-3.0-or-later

//! Undoing a tag change byte for byte: when the librarian writes tags into
//! a copy of a file, it records how to rebuild the original from the copy.
//! Writing the previous tags back would give the same tags, but not always
//! the same bytes (frame order, padding, tag version).
//!
//! The [`Delta`] lists the original's bytes as ranges of the new file
//! (the audio, which a tag change does not touch, even when it shifts) and
//! literal bytes (the old tags). Matches are found like rsync does: the new
//! file is indexed by blocks, the original scanned with a rolling checksum.

use std::collections::HashMap;

/// Files larger than this get no delta: undoing their change writes the
/// previous tags back instead. Both files are read into memory.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Larger deltas are not kept (the `track_tag_changes.restore` column holds
/// 16 MiB, and the database refuses larger packets).
pub const MAX_DELTA_BYTES: usize = 8 * 1024 * 1024;

const BLOCK: usize = 4096;
const MAGIC: &[u8; 4] = b"HRD1";

#[derive(Debug, thiserror::Error)]
pub enum DeltaError {
    #[error("the restore data is damaged")]
    Corrupt,
    #[error("the file is not the one the change wrote")]
    WrongBase,
    #[error("the rebuilt file does not match the original")]
    Mismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    /// Bytes of the new file.
    Copy {
        offset: u64,
        len: u64,
    },
    Literal(Vec<u8>),
}

/// How to rebuild an original file from the file a change wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delta {
    base_len: u64,
    original_len: u64,
    original_hash: [u8; 32],
    ops: Vec<Op>,
}

impl Delta {
    /// The delta that rebuilds `original` from `new`.
    pub fn compute(original: &[u8], new: &[u8]) -> Self {
        let mut ops = Vec::new();
        let prefix = original.iter().zip(new).take_while(|(a, b)| a == b).count();
        let room = original.len().min(new.len()) - prefix;
        let suffix = original
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(room)
            .take_while(|(a, b)| a == b)
            .count();
        push_copy(&mut ops, 0, prefix);
        let original_middle = &original[prefix..original.len() - suffix];
        let new_middle = &new[prefix..new.len() - suffix];
        matches(&mut ops, original_middle, new_middle, prefix);
        push_copy(&mut ops, new.len() - suffix, suffix);
        Self {
            base_len: new.len() as u64,
            original_len: original.len() as u64,
            original_hash: *blake3::hash(original).as_bytes(),
            ops,
        }
    }

    /// Rebuilds the original from `base`, the file the change wrote.
    pub fn apply(&self, base: &[u8]) -> Result<Vec<u8>, DeltaError> {
        if base.len() as u64 != self.base_len {
            return Err(DeltaError::WrongBase);
        }
        let mut out = Vec::with_capacity(usize::try_from(self.original_len).unwrap_or(0));
        for op in &self.ops {
            match op {
                Op::Copy { offset, len } => {
                    let start = usize::try_from(*offset).map_err(|_| DeltaError::Corrupt)?;
                    let len = usize::try_from(*len).map_err(|_| DeltaError::Corrupt)?;
                    let bytes = start
                        .checked_add(len)
                        .and_then(|end| base.get(start..end))
                        .ok_or(DeltaError::Corrupt)?;
                    out.extend_from_slice(bytes);
                }
                Op::Literal(bytes) => out.extend_from_slice(bytes),
            }
        }
        if out.len() as u64 != self.original_len
            || *blake3::hash(&out).as_bytes() != self.original_hash
        {
            return Err(DeltaError::Mismatch);
        }
        Ok(out)
    }

    /// BLAKE3 of the original file.
    pub fn original_hash(&self) -> [u8; 32] {
        self.original_hash
    }

    /// Bytes stored for the original that are not in the new file.
    pub fn literal_bytes(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op {
                Op::Literal(bytes) => bytes.len(),
                Op::Copy { .. } => 0,
            })
            .sum()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.literal_bytes() + self.ops.len() * 17);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.base_len.to_le_bytes());
        out.extend_from_slice(&self.original_len.to_le_bytes());
        out.extend_from_slice(&self.original_hash);
        for op in &self.ops {
            match op {
                Op::Copy { offset, len } => {
                    out.push(0);
                    out.extend_from_slice(&offset.to_le_bytes());
                    out.extend_from_slice(&len.to_le_bytes());
                }
                Op::Literal(bytes) => {
                    out.push(1);
                    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                    out.extend_from_slice(bytes);
                }
            }
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DeltaError> {
        let mut reader = Reader(bytes);
        if reader.take(4)? != MAGIC {
            return Err(DeltaError::Corrupt);
        }
        let base_len = reader.u64()?;
        let original_len = reader.u64()?;
        let original_hash = reader
            .take(32)?
            .try_into()
            .map_err(|_| DeltaError::Corrupt)?;
        let mut ops = Vec::new();
        while !reader.0.is_empty() {
            match reader.take(1)?[0] {
                0 => ops.push(Op::Copy {
                    offset: reader.u64()?,
                    len: reader.u64()?,
                }),
                1 => {
                    let len = usize::try_from(reader.u64()?).map_err(|_| DeltaError::Corrupt)?;
                    ops.push(Op::Literal(reader.take(len)?.to_vec()));
                }
                _ => return Err(DeltaError::Corrupt),
            }
        }
        Ok(Self {
            base_len,
            original_len,
            original_hash,
            ops,
        })
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], DeltaError> {
        if self.0.len() < len {
            return Err(DeltaError::Corrupt);
        }
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(head)
    }

    fn u64(&mut self) -> Result<u64, DeltaError> {
        let bytes = self.take(8)?.try_into().map_err(|_| DeltaError::Corrupt)?;
        Ok(u64::from_le_bytes(bytes))
    }
}

fn push_copy(ops: &mut Vec<Op>, offset: usize, len: usize) {
    if len == 0 {
        return;
    }
    let (offset, len) = (offset as u64, len as u64);
    if let Some(Op::Copy {
        offset: last,
        len: last_len,
    }) = ops.last_mut()
        && *last + *last_len == offset
    {
        *last_len += len;
        return;
    }
    ops.push(Op::Copy { offset, len });
}

fn push_literal(ops: &mut Vec<Op>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Some(Op::Literal(last)) = ops.last_mut() {
        last.extend_from_slice(bytes);
        return;
    }
    ops.push(Op::Literal(bytes.to_vec()));
}

/// The rsync rolling checksum of a block.
#[derive(Clone, Copy)]
struct Rolling {
    a: u32,
    b: u32,
}

impl Rolling {
    fn new(block: &[u8]) -> Self {
        let mut rolling = Self { a: 0, b: 0 };
        let len = block.len() as u32;
        for (index, &byte) in block.iter().enumerate() {
            rolling.a = rolling.a.wrapping_add(u32::from(byte));
            rolling.b = rolling
                .b
                .wrapping_add((len - index as u32).wrapping_mul(u32::from(byte)));
        }
        rolling
    }

    fn roll(&mut self, out: u8, into: u8) {
        self.a = self
            .a
            .wrapping_sub(u32::from(out))
            .wrapping_add(u32::from(into));
        self.b = self
            .b
            .wrapping_sub((BLOCK as u32).wrapping_mul(u32::from(out)))
            .wrapping_add(self.a);
    }

    fn value(self) -> u32 {
        (self.b << 16) | (self.a & 0xffff)
    }
}

/// Appends the ops that rebuild `original` from `new`, found at `base` in
/// the new file.
fn matches(ops: &mut Vec<Op>, original: &[u8], new: &[u8], base: usize) {
    if original.len() < BLOCK || new.len() < BLOCK {
        push_literal(ops, original);
        return;
    }
    let mut index: HashMap<u32, Vec<usize>> = HashMap::new();
    for start in (0..=new.len() - BLOCK).step_by(BLOCK) {
        let sum = Rolling::new(&new[start..start + BLOCK]).value();
        index.entry(sum).or_default().push(start);
    }
    let mut literal_from = 0;
    let mut at = 0;
    let mut rolling = Rolling::new(&original[..BLOCK]);
    while at + BLOCK <= original.len() {
        let window = &original[at..at + BLOCK];
        let found = index.get(&rolling.value()).and_then(|starts| {
            starts
                .iter()
                .copied()
                .find(|&start| &new[start..start + BLOCK] == window)
        });
        if let Some(start) = found {
            // Grow the match both ways.
            let mut before = 0;
            while at - before > literal_from
                && start > before
                && original[at - before - 1] == new[start - before - 1]
            {
                before += 1;
            }
            let mut len = BLOCK;
            while at + len < original.len()
                && start + len < new.len()
                && original[at + len] == new[start + len]
            {
                len += 1;
            }
            push_literal(ops, &original[literal_from..at - before]);
            push_copy(ops, base + start - before, before + len);
            at += len;
            literal_from = at;
            if at + BLOCK <= original.len() {
                rolling = Rolling::new(&original[at..at + BLOCK]);
            }
            continue;
        }
        if at + BLOCK < original.len() {
            rolling.roll(original[at], original[at + BLOCK]);
        }
        at += 1;
    }
    push_literal(ops, &original[literal_from..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise, like compressed audio.
    fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    fn round_trip(original: &[u8], new: &[u8]) -> Delta {
        let delta = Delta::compute(original, new);
        let stored = Delta::from_bytes(&delta.to_bytes()).unwrap();
        assert_eq!(stored, delta);
        assert_eq!(stored.apply(new).unwrap(), original);
        delta
    }

    #[test]
    fn a_new_tag_before_the_audio() {
        let audio = noise(300_000, 1);
        let original = [b"ID3 old tags".as_slice(), &audio].concat();
        let new = [b"ID3 new and much longer tags".as_slice(), &audio].concat();
        let delta = round_trip(&original, &new);
        assert!(delta.literal_bytes() < 64, "{}", delta.literal_bytes());
    }

    #[test]
    fn a_header_field_and_a_tag_at_the_end() {
        // RIFF: the size in the header changes, the tag chunk at the end too.
        let audio = noise(500_000, 2);
        let original = [b"RIFF\x10\x00\x00\x00WAVE".as_slice(), &audio, b"id3 old"].concat();
        let new = [
            b"RIFF\x99\x00\x00\x00WAVE".as_slice(),
            &audio,
            b"id3 new, with a genre",
        ]
        .concat();
        let delta = round_trip(&original, &new);
        assert!(delta.literal_bytes() < 64, "{}", delta.literal_bytes());
    }

    #[test]
    fn shifted_and_reordered_content_is_found() {
        let cover = noise(40_000, 3);
        let audio = noise(200_000, 4);
        let original = [b"tags".as_slice(), &cover, b"more tags", &audio].concat();
        let new = [b"other tags".as_slice(), &cover, &audio].concat();
        let delta = round_trip(&original, &new);
        assert!(
            delta.literal_bytes() < 2 * BLOCK,
            "{}",
            delta.literal_bytes()
        );
    }

    #[test]
    fn small_and_unrelated_files() {
        round_trip(b"", b"");
        round_trip(b"abc", b"");
        round_trip(b"", b"abc");
        round_trip(&noise(10_000, 5), &noise(9_000, 6));
        round_trip(&noise(10_000, 7), &noise(10_000, 7));
    }

    #[test]
    fn a_different_base_is_refused() {
        let original = noise(20_000, 8);
        let mut new = original.clone();
        new[100] ^= 1;
        let delta = Delta::compute(&original, &new);
        assert!(matches!(delta.apply(&new[1..]), Err(DeltaError::WrongBase)));
        let mut other = new.clone();
        other[10_000] ^= 1;
        assert!(matches!(delta.apply(&other), Err(DeltaError::Mismatch)));
        assert!(matches!(
            Delta::from_bytes(b"HRD1short"),
            Err(DeltaError::Corrupt)
        ));
    }
}
