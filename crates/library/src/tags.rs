// SPDX-License-Identifier: AGPL-3.0-or-later

//! The tags Heron reads from files and writes after identification.
//!
//! Only these fields are written; every other tag in the file (genre,
//! comments, lyrics, …) is left alone. Writing a [`Tags`] whose field is
//! `None` removes that field, so writing back the tags read before a change
//! undoes it.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("cannot read the tags: {0}")]
    Read(#[from] lofty::error::FileParseError),
    #[error("cannot write the tags: {0}")]
    Write(#[from] lofty::error::FileEncodingError),
}

/// Longest tag value kept (the width of the `tracks` columns).
pub const MAX_TAG_CHARS: usize = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disc: Option<u32>,
    pub disc_total: Option<u32>,
    /// As written in the file: `1997`, `1997-05-21`, …
    pub date: Option<String>,
    /// Part of a various-artists compilation.
    pub compilation: bool,
    pub musicbrainz: MusicBrainzIds,
}

/// MusicBrainz identifiers, named after the MusicBrainz entities (the tag
/// names differ: Picard's `MUSICBRAINZ_TRACKID` holds the recording ID).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicBrainzIds {
    pub recording: Option<String>,
    pub release_track: Option<String>,
    pub release: Option<String>,
    pub release_group: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
}

impl Tags {
    /// The year of [`Tags::date`], when it starts with one.
    pub fn year(&self) -> Option<u16> {
        let date = self.date.as_deref()?;
        let year = date.get(..4)?;
        year.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| year.parse().ok())
            .flatten()
    }
}

/// What [`read`] finds in a file.
#[derive(Clone, Debug, Default)]
pub struct FileTags {
    pub tags: Tags,
    pub duration_ms: Option<u32>,
    /// The file has embedded cover art.
    pub has_cover: bool,
}

/// Reads the tags and duration of `path`.
pub fn read(path: &Path) -> Result<FileTags, TagError> {
    let tagged = lofty::read_from_path(path)?;
    let duration_ms = u32::try_from(tagged.properties().duration().as_millis())
        .ok()
        .filter(|&ms| ms > 0);
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return Ok(FileTags {
            duration_ms,
            ..FileTags::default()
        });
    };
    let text = |key: ItemKey| tag.get_string(key).and_then(clean);
    let tags = Tags {
        title: tag.title().and_then(|value| clean(&value)),
        artist: tag.artist().and_then(|value| clean(&value)),
        album_artist: text(ItemKey::AlbumArtist),
        album: tag.album().and_then(|value| clean(&value)),
        track: tag.track(),
        track_total: tag.track_total(),
        disc: tag.disk(),
        disc_total: tag.disk_total(),
        date: text(ItemKey::RecordingDate).or_else(|| text(ItemKey::Year)),
        compilation: tag
            .get_string(ItemKey::FlagCompilation)
            .is_some_and(|flag| flag.trim() == "1"),
        musicbrainz: MusicBrainzIds {
            recording: text(ItemKey::MusicBrainzRecordingId),
            release_track: text(ItemKey::MusicBrainzTrackId),
            release: text(ItemKey::MusicBrainzReleaseId),
            release_group: text(ItemKey::MusicBrainzReleaseGroupId),
            artist: text(ItemKey::MusicBrainzArtistId),
            album_artist: text(ItemKey::MusicBrainzReleaseArtistId),
        },
    };
    Ok(FileTags {
        tags,
        duration_ms,
        has_cover: tag.pictures().iter().any(|picture| {
            picture.pic_type() == PictureType::CoverFront
                || picture.pic_type() == PictureType::Other
        }),
    })
}

/// What to do with embedded cover art when writing tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover<'a> {
    Keep,
    /// Embed this JPEG as the front cover, replacing any front cover.
    Embed(&'a [u8]),
    /// Remove the embedded front cover (undoing an [`Cover::Embed`]).
    Remove,
}

/// Writes `tags` into `path` in place. Callers write into a copy and swap
/// it in, so an interruption never leaves a half-written file in the library.
pub fn write(path: &Path, tags: &Tags, cover: Cover<'_>) -> Result<(), TagError> {
    let mut tagged = lofty::read_from_path(path)?;
    let tag_type = tagged.primary_tag_type();
    if tagged.tag(tag_type).is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let Some(tag) = tagged.tag_mut(tag_type) else {
        return Ok(());
    };

    let set_text = |tag: &mut Tag, key: ItemKey, value: &Option<String>| match value {
        Some(value) => {
            tag.insert_text(key, value.clone());
        }
        None => tag.remove_key(key),
    };
    set_text(tag, ItemKey::TrackTitle, &tags.title);
    set_text(tag, ItemKey::TrackArtist, &tags.artist);
    set_text(tag, ItemKey::AlbumArtist, &tags.album_artist);
    set_text(tag, ItemKey::AlbumTitle, &tags.album);
    tag.remove_key(ItemKey::Year);
    set_text(tag, ItemKey::RecordingDate, &tags.date);
    let compilation = tags.compilation.then(|| "1".to_owned());
    set_text(tag, ItemKey::FlagCompilation, &compilation);
    let ids = &tags.musicbrainz;
    set_text(tag, ItemKey::MusicBrainzRecordingId, &ids.recording);
    set_text(tag, ItemKey::MusicBrainzTrackId, &ids.release_track);
    set_text(tag, ItemKey::MusicBrainzReleaseId, &ids.release);
    set_text(tag, ItemKey::MusicBrainzReleaseGroupId, &ids.release_group);
    set_text(tag, ItemKey::MusicBrainzArtistId, &ids.artist);
    set_text(tag, ItemKey::MusicBrainzReleaseArtistId, &ids.album_artist);

    match tags.track {
        Some(track) => tag.set_track(track),
        None => tag.remove_track(),
    }
    match tags.track_total {
        Some(total) => tag.set_track_total(total),
        None => tag.remove_track_total(),
    }
    match tags.disc {
        Some(disc) => tag.set_disk(disc),
        None => tag.remove_disk(),
    }
    match tags.disc_total {
        Some(total) => tag.set_disk_total(total),
        None => tag.remove_disk_total(),
    }

    match cover {
        Cover::Keep => {}
        Cover::Embed(jpeg) => {
            tag.remove_picture_type(PictureType::CoverFront);
            tag.push_picture(
                Picture::unchecked(jpeg.to_vec())
                    .pic_type(PictureType::CoverFront)
                    .mime_type(MimeType::Jpeg)
                    .build(),
            );
        }
        Cover::Remove => tag.remove_picture_type(PictureType::CoverFront),
    }

    Ok(tagged.save_to_path(path, WriteOptions::default())?)
}

/// Trims a tag value and caps its length. Empty values count as missing.
pub fn clean(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('\0').trim();
    (!value.is_empty()).then(|| value.chars().take(MAX_TAG_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for n in 0..8_000 {
            writer.write_sample((n % 100) as i16 * 100).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn full_tags() -> Tags {
        Tags {
            title: Some("So What".into()),
            artist: Some("Miles Davis".into()),
            album_artist: Some("Miles Davis".into()),
            album: Some("Kind of Blue".into()),
            track: Some(1),
            track_total: Some(5),
            disc: Some(1),
            disc_total: Some(1),
            date: Some("1959-08-17".into()),
            compilation: false,
            musicbrainz: MusicBrainzIds {
                recording: Some("7a3b1c2d-0000-4000-8000-000000000001".into()),
                release_track: Some("7a3b1c2d-0000-4000-8000-000000000002".into()),
                release: Some("7a3b1c2d-0000-4000-8000-000000000003".into()),
                release_group: Some("7a3b1c2d-0000-4000-8000-000000000004".into()),
                artist: Some("7a3b1c2d-0000-4000-8000-000000000005".into()),
                album_artist: Some("7a3b1c2d-0000-4000-8000-000000000005".into()),
            },
        }
    }

    #[test]
    fn written_tags_read_back() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.wav", "b.mp3"] {
            let path = dir.path().join(name);
            if name.ends_with("wav") {
                write_wav(&path);
            } else {
                std::fs::copy(
                    concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../analysis/tests/fixtures/silence-1s-sine-3s.mp3"
                    ),
                    &path,
                )
                .unwrap();
            }
            write(&path, &full_tags(), Cover::Keep).unwrap();
            let read = read(&path).unwrap();
            assert_eq!(read.tags, full_tags(), "{name}");
            assert_eq!(read.tags.year(), Some(1959));
            assert!(read.duration_ms.is_some());
        }
    }

    #[test]
    fn writing_previous_tags_undoes_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_wav(&path);
        let previous = Tags {
            title: Some("track 1".into()),
            ..Tags::default()
        };
        write(&path, &previous, Cover::Keep).unwrap();
        write(&path, &full_tags(), Cover::Embed(b"\xff\xd8\xff\xe0fake")).unwrap();
        assert!(read(&path).unwrap().has_cover);

        write(&path, &previous, Cover::Remove).unwrap();
        let read = read(&path).unwrap();
        assert_eq!(read.tags, previous);
        assert!(!read.has_cover);
    }

    #[test]
    fn years_come_from_dates() {
        let tags = |date: &str| Tags {
            date: Some(date.into()),
            ..Tags::default()
        };
        assert_eq!(tags("1997-05-21").year(), Some(1997));
        assert_eq!(tags("1997").year(), Some(1997));
        assert_eq!(tags("May 97").year(), None);
    }
}
