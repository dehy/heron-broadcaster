// SPDX-License-Identifier: AGPL-3.0-or-later

//! The track columns read from a file: the tags shown and edited in the
//! library, and the audio properties. Saved by the scanner for new and
//! changed files, and by the librarian for the files it writes.

use crate::tags::FileTags;

/// What the scanner reads from files, stored in `tracks.details_version`.
/// Tracks read by an older version are read again at the next scan,
/// without being analyzed again.
///
/// - 1: M5b, album artist, genre, year, track and disc numbers, MusicBrainz
///   recording and release IDs, codec, bitrate, sample rate, channels.
pub const DETAILS_VERSION: u8 = 1;

/// Saves the tags and properties of `read` to the track `id`.
pub async fn save(
    executor: impl sqlx::MySqlExecutor<'_>,
    id: u64,
    read: &FileTags,
) -> Result<(), sqlx::Error> {
    let tags = &read.tags;
    let properties = &read.properties;
    let small = |number: Option<u32>| number.and_then(|number| u16::try_from(number).ok());
    let mbid = |id: &Option<String>| {
        id.as_deref()
            .map(|id| id.trim().to_ascii_lowercase())
            .filter(|id| id.len() == 36)
    };
    sqlx::query(
        "UPDATE tracks
         SET title = ?, artist = ?, album = ?, album_artist = ?, genre = ?, year = ?,
             track_number = ?, disc_number = ?, musicbrainz_artist_id = ?,
             musicbrainz_recording_id = ?, musicbrainz_release_id = ?,
             duration_ms = COALESCE(?, duration_ms), codec = ?, bitrate_kbps = ?,
             bitrate_mode = ?, sample_rate = ?, channels = ?, details_version = ?
         WHERE id = ?",
    )
    .bind(&tags.title)
    .bind(&tags.artist)
    .bind(&tags.album)
    .bind(&tags.album_artist)
    .bind(&read.genre)
    .bind(tags.year())
    .bind(small(tags.track))
    .bind(small(tags.disc))
    .bind(tags.musicbrainz.first_artist())
    .bind(mbid(&tags.musicbrainz.recording))
    .bind(mbid(&tags.musicbrainz.release))
    .bind(read.duration_ms)
    .bind(&properties.codec)
    .bind(properties.bitrate_kbps)
    .bind(properties.bitrate_mode.map(|mode| mode.as_str()))
    .bind(properties.sample_rate)
    .bind(properties.channels)
    .bind(DETAILS_VERSION)
    .bind(id)
    .execute(executor)
    .await?;
    Ok(())
}
