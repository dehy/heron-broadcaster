// SPDX-License-Identifier: AGPL-3.0-or-later

//! Identifying tracks: Chromaprint fingerprint → AcoustID (recordings and
//! the releases they are on) → the release picked from the siblings
//! ([`matching`]) → MusicBrainz (the release's tags) → Cover Art Archive.
//!
//! Requests are spaced to respect each service's limits (one per second for
//! MusicBrainz, three for AcoustID) and carry a User-Agent that identifies
//! Heron, as MusicBrainz requires.

pub mod matching;
pub mod ratelimit;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use heron_analysis::Chromaprint;
use heron_core::library::AlbumKind;
use heron_library::{MusicBrainzIds, Tags};
use matching::{Candidate, Choice, FileMatches};
use ratelimit::RateLimiter;
use reqwest::StatusCode;
use serde::Deserialize;

/// MusicBrainz ID of the "Various Artists" artist.
const VARIOUS_ARTISTS_ID: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

/// Attempts at a request the service answered with 503 (slow down) or 429:
/// a busy MusicBrainz often refuses several in a row.
const ATTEMPTS: u32 = 6;
/// The longest wait between two of them.
const MAX_BACK_OFF: Duration = Duration::from_secs(8);

/// Base URLs, configurable for mirrors and tests.
#[derive(Clone, Debug)]
pub struct Endpoints {
    /// The AcoustID lookup URL.
    pub acoustid: String,
    /// The MusicBrainz web service root, ending with `/ws/2`.
    pub musicbrainz: String,
    pub cover_art: String,
}

/// The rate limiter of a service, shared by every [`Identifier`] of the
/// process: the stager, the tasks and the background passes each make
/// their own, and together they must keep to the service's rate.
fn shared_limiter(url: &str, interval: Duration) -> Arc<RateLimiter> {
    static LIMITERS: LazyLock<std::sync::Mutex<HashMap<String, Arc<RateLimiter>>>> =
        LazyLock::new(Default::default);
    // A poisoned map still holds valid limiters.
    let mut limiters = LIMITERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    limiters
        .entry(url.to_owned())
        .or_insert_with(|| Arc::new(RateLimiter::new(interval)))
        .clone()
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            acoustid: "https://api.acoustid.org/v2/lookup".into(),
            musicbrainz: "https://musicbrainz.org/ws/2".into(),
            cover_art: "https://coverartarchive.org".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentifyError {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{service} answered {status}: {message}")]
    Service {
        service: &'static str,
        status: u16,
        message: String,
    },
}

impl IdentifyError {
    /// A refusal that trying again will not change, such as an invalid
    /// key: 4xx except 429 (too many requests).
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Service { status, .. } if (400..500).contains(status) && *status != 429)
    }
}

/// What is known about a file to identify.
#[derive(Clone, Debug, Default)]
pub struct FileToIdentify {
    pub chromaprint: Option<Chromaprint>,
    /// Whole seconds, as AcoustID wants.
    pub duration_s: Option<u32>,
    pub album_tag: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Identified {
    Match {
        tags: Box<Tags>,
        score: f64,
        recording_mbid: String,
        release_mbid: String,
        candidates: Vec<Candidate>,
    },
    Review {
        reason: String,
        candidates: Vec<Candidate>,
    },
}

pub struct Identifier {
    http: reqwest::Client,
    endpoints: Endpoints,
    key: String,
    min_score: f64,
    acoustid: Arc<RateLimiter>,
    musicbrainz: Arc<RateLimiter>,
    cover_art: Arc<RateLimiter>,
}

impl Identifier {
    pub fn new(endpoints: Endpoints, key: String, min_score: f64) -> Result<Self, IdentifyError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "HeronBroadcaster/",
                env!("CARGO_PKG_VERSION"),
                " ( https://heronbroadcaster.org )"
            ))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            key,
            min_score,
            acoustid: shared_limiter(&endpoints.acoustid, Duration::from_millis(334)),
            // MusicBrainz allows one request a second over a rolling window:
            // exactly one a second gets refused (503) when the passes are busy.
            musicbrainz: shared_limiter(&endpoints.musicbrainz, Duration::from_millis(1200)),
            cover_art: shared_limiter(&endpoints.cover_art, Duration::from_secs(1)),
            endpoints,
        })
    }

    /// Identifies files dropped together. Errors are transient (network,
    /// service down): try again later.
    pub async fn identify(
        &self,
        files: &[FileToIdentify],
    ) -> Result<Vec<Identified>, IdentifyError> {
        let mut matches = Vec::with_capacity(files.len());
        let mut unmatched_reasons = Vec::with_capacity(files.len());
        for file in files {
            let (candidates, reason) = match (&file.chromaprint, file.duration_s) {
                (Some(chromaprint), Some(duration)) => {
                    let results = self.lookup(chromaprint, duration).await?;
                    let best = results
                        .iter()
                        .map(|c| c.score)
                        .fold(None, |best: Option<f64>, s| {
                            Some(best.map_or(s, |b| b.max(s)))
                        });
                    let confident: Vec<Candidate> = results
                        .into_iter()
                        .filter(|c| c.score >= self.min_score)
                        .collect();
                    let reason = match best {
                        None => "not found on AcoustID".to_owned(),
                        Some(best) => format!(
                            "best AcoustID score {best:.2} is below {:.2}",
                            self.min_score
                        ),
                    };
                    (confident, reason)
                }
                (None, _) => (
                    Vec::new(),
                    "the audio could not be fingerprinted".to_owned(),
                ),
                (_, None) => (Vec::new(), "unknown duration".to_owned()),
            };
            matches.push(FileMatches {
                album_tag: file.album_tag.clone(),
                candidates,
            });
            unmatched_reasons.push(reason);
        }

        let choices = matching::choose(&matches);
        let mut releases: HashMap<String, Option<MbRelease>> = HashMap::new();
        let mut identified = Vec::with_capacity(files.len());
        for ((choice, file), unmatched) in choices.into_iter().zip(matches).zip(unmatched_reasons) {
            let candidates = file.candidates;
            let (release_id, recording_id, score, disc, track) = match choice {
                Choice::Review(reason) => {
                    let reason = if candidates.is_empty() {
                        unmatched
                    } else {
                        reason
                    };
                    identified.push(Identified::Review { reason, candidates });
                    continue;
                }
                Choice::Release {
                    release_id,
                    recording_id,
                    score,
                    disc,
                    track,
                } => (release_id, recording_id, score, disc, track),
            };
            if !releases.contains_key(&release_id) {
                let release = self.release(&release_id).await?;
                releases.insert(release_id.clone(), release);
            }
            let tags = releases[&release_id]
                .as_ref()
                .and_then(|release| release.tags(&recording_id, disc, track));
            identified.push(match tags {
                Some(tags) => Identified::Match {
                    tags: Box::new(tags),
                    score,
                    recording_mbid: recording_id,
                    release_mbid: release_id,
                    candidates,
                },
                None => Identified::Review {
                    reason: "the MusicBrainz release does not list the recording".into(),
                    candidates,
                },
            });
        }
        Ok(identified)
    }

    /// AcoustID matches of a fingerprint, flattened to one candidate per
    /// place the recording appears on a release.
    async fn lookup(
        &self,
        chromaprint: &Chromaprint,
        duration_s: u32,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let form = [
            ("client", self.key.as_str()),
            ("format", "json"),
            ("meta", "recordings releasegroups releases tracks compress"),
            ("duration", &duration_s.to_string()),
            ("fingerprint", &chromaprint.encoded()),
        ];
        let response: AcoustidResponse = self
            .send("AcoustID", &self.acoustid, || {
                self.http.post(&self.endpoints.acoustid).form(&form)
            })
            .await?
            .json()
            .await?;
        if response.status != "ok" {
            let error = response.error.unwrap_or_default();
            return Err(IdentifyError::Service {
                service: "AcoustID",
                status: 200,
                message: format!("error {}: {}", error.code, error.message),
            });
        }
        let mut candidates = Vec::new();
        for result in response.results {
            for recording in result.recordings {
                for group in &recording.releasegroups {
                    for release in &group.releases {
                        let placements: Vec<(Option<u32>, Option<u32>)> = release
                            .mediums
                            .iter()
                            .flat_map(|medium| {
                                medium
                                    .tracks
                                    .iter()
                                    .map(|track| (medium.position, track.position))
                            })
                            .collect();
                        let placements = if placements.is_empty() {
                            vec![(None, None)]
                        } else {
                            placements
                        };
                        for (disc, track) in placements {
                            candidates.push(Candidate {
                                score: result.score,
                                recording_id: recording.id.clone(),
                                title: recording.title.clone(),
                                artist: recording.artist(),
                                release_id: release.id.clone(),
                                release_title: release
                                    .title
                                    .clone()
                                    .or_else(|| group.title.clone()),
                                release_group_id: Some(group.id.clone()),
                                release_group_title: group.title.clone(),
                                date: release.date.as_ref().and_then(AcoustidDate::formatted),
                                country: release.country.clone(),
                                disc,
                                track,
                                length_ms: recording
                                    .duration
                                    .map(|seconds| (seconds * 1000.0).round() as u32),
                                album_kind: Some(AlbumKind::of(
                                    group.kind.as_deref(),
                                    &group.secondary_types,
                                )),
                                ..Candidate::default()
                            });
                        }
                    }
                }
            }
        }
        Ok(candidates)
    }

    /// A MusicBrainz release with its tracks. `None` if MusicBrainz does
    /// not know it (merged or removed since AcoustID indexed it).
    async fn release(&self, id: &str) -> Result<Option<MbRelease>, IdentifyError> {
        let url = format!("{}/release/{id}", self.endpoints.musicbrainz);
        let response = self
            .send("MusicBrainz", &self.musicbrainz, || {
                self.http.get(&url).query(&[
                    ("inc", "recordings artist-credits release-groups"),
                    ("fmt", "json"),
                ])
            })
            .await;
        match response {
            Ok(response) => Ok(Some(response.json().await?)),
            Err(IdentifyError::Service { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The tags of `recording_id` on a release picked by hand (on the review
    /// screen). `None` if MusicBrainz does not know the release, or the
    /// release does not list the recording.
    pub async fn release_tags(
        &self,
        release_id: &str,
        recording_id: &str,
        disc: Option<u32>,
        track: Option<u32>,
    ) -> Result<Option<Tags>, IdentifyError> {
        Ok(self
            .release(release_id)
            .await?
            .and_then(|release| release.tags(recording_id, disc, track)))
    }

    /// The programming details of the recordings on a release: album,
    /// record label, catalog number, ISRC, first release year and genres,
    /// by recording ID. `None` if MusicBrainz does not know the release.
    pub async fn release_details(
        &self,
        release_id: &str,
    ) -> Result<Option<HashMap<String, RecordingDetails>>, IdentifyError> {
        let url = format!("{}/release/{release_id}", self.endpoints.musicbrainz);
        let response = self
            .send("MusicBrainz", &self.musicbrainz, || {
                self.http.get(&url).query(&[
                    ("inc", "labels recordings isrcs genres release-groups"),
                    ("fmt", "json"),
                ])
            })
            .await;
        let release: MbReleaseDetails = match response {
            Ok(response) => response.json().await?,
            Err(IdentifyError::Service { status: 404, .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(release.recordings()))
    }

    /// The artists `id` is related to on MusicBrainz: relationship type
    /// (`member of band`, `is person`, …) and the other artist's ID, in
    /// MusicBrainz's direction. Empty if MusicBrainz does not know it.
    pub async fn artist_relations(
        &self,
        id: &str,
    ) -> Result<Vec<(String, String, bool)>, IdentifyError> {
        let url = format!("{}/artist/{id}", self.endpoints.musicbrainz);
        let response = self
            .send("MusicBrainz", &self.musicbrainz, || {
                self.http
                    .get(&url)
                    .query(&[("inc", "artist-rels"), ("fmt", "json")])
            })
            .await;
        let artist: MbArtistRelations = match response {
            Ok(response) => response.json().await?,
            Err(IdentifyError::Service { status: 404, .. }) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        Ok(artist
            .relations
            .into_iter()
            .filter_map(|relation| {
                let other = relation.artist?;
                Some((relation.kind, other.id, relation.direction == "backward"))
            })
            .collect())
    }

    /// The Wikidata item of the artist `id` (`Q42`), from its MusicBrainz
    /// URL relationships. `None` when MusicBrainz has no Wikidata link for
    /// the artist, or does not know it.
    pub async fn artist_wikidata(&self, id: &str) -> Result<Option<String>, IdentifyError> {
        let url = format!("{}/artist/{id}", self.endpoints.musicbrainz);
        let response = self
            .send("MusicBrainz", &self.musicbrainz, || {
                self.http
                    .get(&url)
                    .query(&[("inc", "url-rels"), ("fmt", "json")])
            })
            .await;
        let artist: MbArtistUrls = match response {
            Ok(response) => response.json().await?,
            Err(IdentifyError::Service { status: 404, .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(artist.wikidata())
    }

    /// The release's front cover (500 px JPEG), if it has one.
    pub async fn cover(&self, release_id: &str) -> Result<Option<Vec<u8>>, IdentifyError> {
        let url = format!(
            "{}/release/{release_id}/front-500",
            self.endpoints.cover_art
        );
        match self
            .send("Cover Art Archive", &self.cover_art, || self.http.get(&url))
            .await
        {
            Ok(response) => Ok(Some(response.bytes().await?.to_vec())),
            Err(IdentifyError::Service { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The address of a release's front cover thumbnail (250 px), for
    /// people to look at.
    pub fn cover_thumbnail_url(&self, release_id: &str) -> String {
        format!(
            "{}/release/{release_id}/front-250",
            self.endpoints.cover_art
        )
    }

    /// Searches MusicBrainz for releases titled `album` by `artist`, best
    /// first, with MusicBrainz's score (0 to 100).
    pub async fn search_releases(
        &self,
        album: &str,
        artist: &str,
    ) -> Result<Vec<ReleaseHit>, IdentifyError> {
        let url = format!("{}/release", self.endpoints.musicbrainz);
        let query = format!(
            "release:{} AND artist:{}",
            lucene_phrase(album),
            lucene_phrase(artist)
        );
        let response = self
            .send("MusicBrainz", &self.musicbrainz, || {
                self.http.get(&url).query(&[
                    ("query", query.as_str()),
                    ("limit", "10"),
                    ("fmt", "json"),
                ])
            })
            .await?;
        let found: MbReleaseSearch = response.json().await?;
        let mut hits: Vec<ReleaseHit> = found
            .releases
            .into_iter()
            .map(|release| ReleaseHit {
                artist: credit_text(&release.artist_credit),
                id: release.id,
                title: release.title,
                date: release.date.filter(|date| !date.is_empty()),
                country: release.country.filter(|country| !country.is_empty()),
                score: release.score.min(100) as u8,
            })
            .collect();
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        Ok(hits)
    }

    /// Searches MusicBrainz for recordings titled `title` by `artist`, on
    /// official albums of these `kinds` (none: studio albums): one
    /// candidate per album, studio albums first, then the earliest. Those
    /// titled `album` come first.
    pub async fn search_recordings(
        &self,
        title: &str,
        artist: &str,
        album: Option<&str>,
        kinds: &[AlbumKind],
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let kinds = if kinds.is_empty() {
            &[AlbumKind::Studio][..]
        } else {
            kinds
        };
        let title = search_title(title);
        if let Some(asked) = album.and_then(AlbumId::parse) {
            return self.search_album(&asked, &title).await;
        }
        let base = format!(
            "recording:{} AND artist:{}{}",
            lucene_phrase(&title),
            lucene_phrase(artist),
            kinds_query(kinds)
        );
        let album = album.map(str::trim).filter(|album| !album.is_empty());
        // On its album first: a well-known song is on hundreds of releases.
        if let Some(album) = album {
            let query = format!("{base} AND release:{}", lucene_phrase(album));
            let found = album_candidates(
                self.search_recording_query(&query).await?,
                &title,
                Some(album),
                kinds,
                Album::Any,
            );
            // `release:"Post"` also finds "Post Live": only that album.
            let wanted = album.to_lowercase();
            if found.iter().any(|candidate| {
                candidate
                    .release_group_title
                    .as_deref()
                    .is_some_and(|name| name.to_lowercase() == wanted)
            }) {
                return Ok(found);
            }
        }
        let found = album_candidates(
            self.search_recording_query(&base).await?,
            &title,
            album,
            kinds,
            Album::Any,
        );
        // None of these kinds: every kind, the results show which.
        const EVERY: [AlbumKind; 5] = EVERY_KIND;
        if !found.is_empty() || EVERY.iter().all(|kind| kinds.contains(kind)) {
            return Ok(found);
        }
        let base = format!(
            "recording:{} AND artist:{}",
            lucene_phrase(&title),
            lucene_phrase(artist)
        );
        Ok(album_candidates(
            self.search_recording_query(&base).await?,
            &title,
            album,
            &EVERY,
            Album::Any,
        ))
    }

    /// The song `title` on the album `asked` (a release group, or one of its
    /// releases), whatever its kind; its tracklist when no track of it is
    /// titled so, to pick the song from.
    async fn search_album(
        &self,
        asked: &AlbumId,
        title: &str,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let on_it = asked.query();
        let query = format!("{on_it} AND recording:{}", lucene_phrase(title));
        let found = album_candidates(
            self.search_recording_query(&query).await?,
            title,
            None,
            &EVERY_KIND,
            Album::Only(asked),
        );
        if !found.is_empty() {
            return Ok(found);
        }
        Ok(album_candidates(
            self.search_recording_query(&on_it).await?,
            title,
            None,
            &EVERY_KIND,
            Album::Tracklist(asked),
        ))
    }

    async fn search_recording_query(
        &self,
        query: &str,
    ) -> Result<MbRecordingSearch, IdentifyError> {
        let url = format!("{}/recording", self.endpoints.musicbrainz);
        let response = self
            .send("MusicBrainz", &self.musicbrainz, || {
                self.http
                    .get(&url)
                    .query(&[("query", query), ("limit", "100"), ("fmt", "json")])
            })
            .await?;
        Ok(response.json().await?)
    }

    /// Sends a request in its rate-limit slot, retrying when the service
    /// asks to slow down.
    async fn send(
        &self,
        service: &'static str,
        limiter: &RateLimiter,
        request: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, IdentifyError> {
        let mut attempt = 1;
        loop {
            limiter.wait().await;
            let response = request().send().await;
            limiter.done().await;
            let response = response?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            let slow_down = status == StatusCode::SERVICE_UNAVAILABLE
                || status == StatusCode::TOO_MANY_REQUESTS;
            if slow_down && attempt < ATTEMPTS {
                tracing::info!(service, %status, "rate limited, slowing down");
                limiter
                    .back_off(Duration::from_secs(2_u64.pow(attempt)).min(MAX_BACK_OFF))
                    .await;
                attempt += 1;
                continue;
            }
            let message: String = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect();
            return Err(IdentifyError::Service {
                service,
                status: status.as_u16(),
                message,
            });
        }
    }
}

// AcoustID lookup responses, as requested with
// `meta=recordings releasegroups releases tracks`.

#[derive(Debug, Deserialize)]
struct AcoustidResponse {
    status: String,
    #[serde(default)]
    results: Vec<AcoustidResult>,
    error: Option<AcoustidError>,
}

#[derive(Debug, Default, Deserialize)]
struct AcoustidError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct AcoustidResult {
    score: f64,
    #[serde(default)]
    recordings: Vec<AcoustidRecording>,
}

#[derive(Debug, Deserialize)]
struct AcoustidRecording {
    id: String,
    title: Option<String>,
    #[serde(default)]
    artists: Vec<AcoustidArtist>,
    /// In seconds.
    duration: Option<f64>,
    #[serde(default)]
    releasegroups: Vec<AcoustidReleaseGroup>,
}

#[derive(Debug, Deserialize)]
struct AcoustidArtist {
    name: String,
    #[serde(default)]
    joinphrase: Option<String>,
}

impl AcoustidRecording {
    /// The artists as credited (`A feat. B`).
    fn artist(&self) -> Option<String> {
        let text: String = self
            .artists
            .iter()
            .enumerate()
            .map(|(index, artist)| {
                let join = artist.joinphrase.clone().unwrap_or_else(|| {
                    if index + 1 < self.artists.len() {
                        ", "
                    } else {
                        ""
                    }
                    .to_owned()
                });
                format!("{}{join}", artist.name)
            })
            .collect();
        (!text.is_empty()).then_some(text)
    }
}

#[derive(Debug, Deserialize)]
struct AcoustidReleaseGroup {
    id: String,
    title: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "secondarytypes", default)]
    secondary_types: Vec<String>,
    #[serde(default)]
    releases: Vec<AcoustidRelease>,
}

#[derive(Debug, Deserialize)]
struct AcoustidRelease {
    id: String,
    title: Option<String>,
    country: Option<String>,
    date: Option<AcoustidDate>,
    #[serde(default)]
    mediums: Vec<AcoustidMedium>,
}

#[derive(Debug, Deserialize)]
struct AcoustidDate {
    year: Option<u32>,
    month: Option<u32>,
    day: Option<u32>,
}

impl AcoustidDate {
    fn formatted(&self) -> Option<String> {
        let year = self.year?;
        Some(match (self.month, self.day) {
            (Some(month), Some(day)) => format!("{year:04}-{month:02}-{day:02}"),
            (Some(month), None) => format!("{year:04}-{month:02}"),
            _ => format!("{year:04}"),
        })
    }
}

#[derive(Debug, Deserialize)]
struct AcoustidMedium {
    position: Option<u32>,
    #[serde(default)]
    tracks: Vec<AcoustidTrack>,
}

#[derive(Debug, Deserialize)]
struct AcoustidTrack {
    position: Option<u32>,
}

/// A release found by a MusicBrainz search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseHit {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub date: Option<String>,
    pub country: Option<String>,
    /// 0 to 100.
    pub score: u8,
}

/// A phrase for a Lucene query, quoted.
fn lucene_phrase(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// An artist credit as printed.
fn credit_text(credits: &[MbCredit]) -> String {
    credits
        .iter()
        .map(|credit| format!("{}{}", credit.name, credit.joinphrase))
        .collect()
}

/// Words of an edition, not of the song: `We Will Rock You (Remastered
/// 2011)` is `We Will Rock You` on MusicBrainz.
const EDITION_WORDS: [&str; 5] = ["remaster", "deluxe", "bonus", "anniversary", "reissue"];

/// `title` without the edition notes a file's tags often carry, in
/// brackets or after a dash: what MusicBrainz names the recording.
fn search_title(title: &str) -> String {
    let edition = |text: &str| {
        let text = text.to_lowercase();
        EDITION_WORDS.iter().any(|word| text.contains(word))
    };
    let mut kept = String::new();
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[']) {
        let close = if rest[open..].starts_with('(') {
            ')'
        } else {
            ']'
        };
        let Some(length) = rest[open..].find(close) else {
            break;
        };
        let group = &rest[open..=open + length];
        kept.push_str(&rest[..open]);
        if !edition(group) {
            kept.push_str(group);
        }
        rest = &rest[open + length + 1..];
    }
    kept.push_str(rest);
    let kept = match kept.rfind(" - ") {
        Some(dash) if edition(&kept[dash..]) => kept[..dash].to_owned(),
        _ => kept,
    };
    let cleaned = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        title.trim().to_owned()
    } else {
        cleaned
    }
}

/// The query clauses that keep the recordings on official albums of
/// `kinds`. A recording matches a clause when any of its releases does,
/// so the kinds are checked again on each release: only "live" in a
/// recording's disambiguation tells a live recording apart.
fn kinds_query(kinds: &[AlbumKind]) -> String {
    let mut query = " AND status:official".to_owned();
    if !kinds.contains(&AlbumKind::Other) {
        let mut primary = Vec::new();
        if kinds.iter().any(|kind| {
            matches!(
                kind,
                AlbumKind::Studio | AlbumKind::Live | AlbumKind::Compilation
            )
        }) {
            primary.push("album");
        }
        if kinds.contains(&AlbumKind::Single) {
            primary.extend(["single", "ep"]);
        }
        query.push_str(&format!(" AND primarytype:({})", primary.join(" OR ")));
    }
    if !kinds.contains(&AlbumKind::Live) {
        query.push_str(" AND NOT comment:live");
    }
    query
}

/// At most this many albums from a search.
const SEARCH_CANDIDATES: usize = 25;

const EVERY_KIND: [AlbumKind; 5] = [
    AlbumKind::Studio,
    AlbumKind::Live,
    AlbumKind::Compilation,
    AlbumKind::Single,
    AlbumKind::Other,
];

/// An album given by its MusicBrainz ID or address, instead of its title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlbumId {
    /// `https://musicbrainz.org/release-group/…`
    ReleaseGroup(String),
    /// `https://musicbrainz.org/release/…`: that edition only.
    Release(String),
    /// An ID alone: a release group or a release.
    Either(String),
}

impl AlbumId {
    /// The album `text` names, if it is a MusicBrainz ID or the address of
    /// a release group or a release (`?…` and `#…` ignored).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let path = text.split(['?', '#']).next().unwrap_or_default();
        let mut parts = path.trim_end_matches('/').rsplit('/');
        let id = parts.next()?.to_ascii_lowercase();
        if !is_mbid(&id) {
            return None;
        }
        match parts.next() {
            None if path == text && !text.contains('/') => Some(Self::Either(id)),
            Some("release-group") => Some(Self::ReleaseGroup(id)),
            Some("release") => Some(Self::Release(id)),
            _ => None,
        }
    }

    /// The search clause for its recordings.
    fn query(&self) -> String {
        match self {
            Self::ReleaseGroup(id) => format!("rgid:{id}"),
            Self::Release(id) => format!("reid:{id}"),
            Self::Either(id) => format!("(rgid:{id} OR reid:{id})"),
        }
    }
}

/// A MusicBrainz ID: 8-4-4-4-12 hexadecimal digits.
fn is_mbid(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Which albums of a search are kept.
#[derive(Clone, Copy)]
enum Album<'a> {
    Any,
    /// The one asked for, one candidate.
    Only(&'a AlbumId),
    /// The one asked for, a candidate per recording on it.
    Tracklist(&'a AlbumId),
}

impl Album<'_> {
    fn has(self, group: &str, release: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Only(asked) | Self::Tracklist(asked) => match asked {
                AlbumId::ReleaseGroup(id) => id == group,
                AlbumId::Release(id) => id == release,
                AlbumId::Either(id) => id == group || id == release,
            },
        }
    }
}

/// The albums of a recording search, one candidate each: the edition of
/// the recording most likely the song's original (named `title`, with no
/// disambiguation, on the most releases), the earliest. Albums titled
/// `album` first, then by kind (studio first), then the earliest.
fn album_candidates(
    found: MbRecordingSearch,
    title: &str,
    album: Option<&str>,
    kinds: &[AlbumKind],
    asked: Album<'_>,
) -> Vec<Candidate> {
    struct Found {
        best: (bool, usize, Candidate),
        earliest: Option<String>,
        editions: HashSet<String>,
    }
    let title = title.to_lowercase();
    let album = album.map(str::to_lowercase);
    let mut albums: HashMap<String, Found> = HashMap::new();
    for recording in found.recordings {
        let score = f64::from(recording.score.min(100)) / 100.0;
        let artist = Some(credit_text(&recording.artist_credit)).filter(|text| !text.is_empty());
        let plain = recording.disambiguation.is_empty()
            && recording
                .title
                .as_deref()
                .is_some_and(|name| name.to_lowercase() == title);
        let releases = recording.releases.len();
        for release in recording.releases {
            if release
                .status
                .as_deref()
                .is_some_and(|status| !status.eq_ignore_ascii_case("official"))
            {
                continue;
            }
            let Some(group) = release.release_group else {
                continue;
            };
            let kind = AlbumKind::of(group.primary_type.as_deref(), &group.secondary_types);
            if !kinds.contains(&kind) || !asked.has(&group.id, &release.id) {
                continue;
            }
            let medium = release.media.first();
            let date = release.date.filter(|date| !date.is_empty());
            let candidate = Candidate {
                score,
                recording_id: recording.id.clone(),
                title: recording.title.clone(),
                artist: artist.clone(),
                release_id: release.id.clone(),
                release_title: Some(release.title),
                release_group_id: Some(group.id.clone()),
                release_group_title: group.title,
                date: date.clone(),
                country: release.country.filter(|country| !country.is_empty()),
                disc: medium.and_then(|medium| medium.position),
                track: medium.and_then(|medium| medium.track_offset.map(|offset| offset + 1)),
                length_ms: recording.length,
                album_kind: Some(kind),
                format: medium.and_then(|medium| medium.format.clone()),
                track_count: release.track_count,
                ..Candidate::default()
            };
            let key = match asked {
                Album::Tracklist(_) => format!("{}/{}", group.id, recording.id),
                Album::Any | Album::Only(_) => group.id,
            };
            let entry = albums.entry(key).or_insert_with(|| Found {
                best: (plain, releases, candidate.clone()),
                earliest: None,
                editions: HashSet::new(),
            });
            entry.editions.insert(release.id);
            if let Some(date) = &date
                && entry
                    .earliest
                    .as_ref()
                    .is_none_or(|earliest| date < earliest)
            {
                entry.earliest = Some(date.clone());
            }
            let (best_plain, best_releases, best) = &entry.best;
            let better = (plain, releases)
                .cmp(&(*best_plain, *best_releases))
                .then_with(|| earlier(candidate.date.as_ref(), best.date.as_ref()))
                .is_gt();
            if better {
                entry.best = (plain, releases, candidate);
            }
        }
    }
    let mut candidates: Vec<Candidate> = albums
        .into_values()
        .map(|album| {
            let (_, _, mut candidate) = album.best;
            candidate.album_date = album.earliest;
            candidate.editions = u32::try_from(album.editions.len()).ok();
            candidate
        })
        .collect();
    let on_album = |candidate: &Candidate| {
        album.as_deref().is_some_and(|album| {
            candidate
                .release_group_title
                .as_deref()
                .is_some_and(|name| name.to_lowercase() == album)
        })
    };
    if let Album::Tracklist(_) = asked {
        // In the album's order, every track: the song is picked from it.
        candidates.sort_by_key(|candidate| (candidate.disc, candidate.track));
    } else {
        candidates.sort_by(|a, b| {
            on_album(b)
                .cmp(&on_album(a))
                .then(a.album_kind.cmp(&b.album_kind))
                .then_with(|| earlier(b.album_date.as_ref(), a.album_date.as_ref()))
                .then(b.score.total_cmp(&a.score))
        });
        candidates.truncate(SEARCH_CANDIDATES);
    }
    // A search's score says how well the words match, not which album is
    // right: every album gets the best one, so the order above stays.
    let best = candidates.iter().map(|c| c.score).fold(0.0, f64::max);
    for candidate in &mut candidates {
        candidate.score = best;
    }
    candidates
}

/// `Greater` when `a` is earlier than `b`; a missing date is the latest.
fn earlier(a: Option<&String>, b: Option<&String>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(a),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[derive(Debug, Deserialize)]
struct MbRecordingSearch {
    #[serde(default)]
    recordings: Vec<MbSearchRecording>,
}

#[derive(Debug, Deserialize)]
struct MbSearchRecording {
    id: String,
    title: Option<String>,
    #[serde(default)]
    disambiguation: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbCredit>,
    #[serde(default)]
    score: u32,
    length: Option<u32>,
    #[serde(default)]
    releases: Vec<MbSearchRecordingRelease>,
}

#[derive(Debug, Deserialize)]
struct MbSearchRecordingRelease {
    id: String,
    title: String,
    status: Option<String>,
    date: Option<String>,
    country: Option<String>,
    #[serde(rename = "track-count")]
    track_count: Option<u32>,
    #[serde(rename = "release-group")]
    release_group: Option<MbSearchReleaseGroup>,
    #[serde(default)]
    media: Vec<MbSearchMedium>,
}

#[derive(Debug, Deserialize)]
struct MbSearchReleaseGroup {
    id: String,
    title: Option<String>,
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
    #[serde(rename = "secondary-types", default)]
    secondary_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MbSearchMedium {
    position: Option<u32>,
    format: Option<String>,
    #[serde(rename = "track-offset")]
    track_offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseSearch {
    #[serde(default)]
    releases: Vec<MbSearchRelease>,
}

#[derive(Debug, Deserialize)]
struct MbSearchRelease {
    id: String,
    #[serde(default)]
    score: u32,
    title: String,
    date: Option<String>,
    country: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbCredit>,
}

// MusicBrainz release lookups (`inc=recordings artist-credits release-groups`).

#[derive(Debug, Deserialize)]
struct MbRelease {
    id: String,
    title: String,
    date: Option<String>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbCredit>,
    #[serde(rename = "release-group")]
    release_group: Option<MbReleaseGroup>,
    #[serde(default)]
    media: Vec<MbMedium>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseGroup {
    id: String,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MbMedium {
    position: Option<u32>,
    #[serde(rename = "track-count")]
    track_count: Option<u32>,
    #[serde(default)]
    tracks: Vec<MbTrack>,
}

#[derive(Debug, Deserialize)]
struct MbTrack {
    id: String,
    position: Option<u32>,
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<MbCredit>,
    recording: MbRecording,
}

#[derive(Debug, Deserialize)]
struct MbRecording {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MbCredit {
    name: String,
    #[serde(default)]
    joinphrase: String,
    artist: MbArtist,
}

#[derive(Debug, Deserialize)]
struct MbArtist {
    id: String,
}

// MusicBrainz release lookups for programming details
// (`inc=labels recordings isrcs genres release-groups`).

/// What MusicBrainz says about a recording on a release.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordingDetails {
    pub album: Option<String>,
    pub record_label: Option<String>,
    pub catalog_number: Option<String>,
    pub isrc: Option<String>,
    pub first_release_year: Option<u16>,
    /// Most voted first.
    pub genres: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseDetails {
    title: Option<String>,
    date: Option<String>,
    #[serde(rename = "label-info", default)]
    label_info: Vec<MbLabelInfo>,
    #[serde(default)]
    genres: Vec<MbGenre>,
    #[serde(rename = "release-group")]
    release_group: Option<MbGroupDetails>,
    #[serde(default)]
    media: Vec<MbMediumDetails>,
}

#[derive(Debug, Deserialize)]
struct MbLabelInfo {
    #[serde(rename = "catalog-number")]
    catalog_number: Option<String>,
    label: Option<MbLabel>,
}

#[derive(Debug, Deserialize)]
struct MbLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct MbGenre {
    name: String,
    #[serde(default)]
    count: u32,
}

#[derive(Debug, Deserialize)]
struct MbGroupDetails {
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(default)]
    genres: Vec<MbGenre>,
}

#[derive(Debug, Deserialize)]
struct MbMediumDetails {
    #[serde(default)]
    tracks: Vec<MbTrackDetails>,
}

#[derive(Debug, Deserialize)]
struct MbTrackDetails {
    recording: MbRecordingDetails,
}

#[derive(Debug, Deserialize)]
struct MbRecordingDetails {
    id: String,
    #[serde(default)]
    isrcs: Vec<String>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(default)]
    genres: Vec<MbGenre>,
}

/// The year a date (`1997-05-21`, `1997`) starts with.
fn year(date: Option<&str>) -> Option<u16> {
    let year = date?.get(..4)?;
    year.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| year.parse().ok())
        .flatten()
        .filter(|&year| year > 0)
}

/// Genre names, most voted first, without duplicates.
fn genre_names<'a>(lists: impl IntoIterator<Item = &'a [MbGenre]>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for list in lists {
        let mut list: Vec<&MbGenre> = list.iter().collect();
        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        for genre in list {
            if !names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&genre.name))
            {
                names.push(genre.name.clone());
            }
        }
    }
    names.truncate(10);
    names
}

impl MbReleaseDetails {
    fn recordings(&self) -> HashMap<String, RecordingDetails> {
        let label = self.label_info.iter().find(|info| info.label.is_some());
        let group = self.release_group.as_ref();
        let release_year = year(group.and_then(|group| group.first_release_date.as_deref()))
            .or_else(|| year(self.date.as_deref()));
        let empty: &[MbGenre] = &[];
        self.media
            .iter()
            .flat_map(|medium| &medium.tracks)
            .map(|track| {
                let recording = &track.recording;
                let details = RecordingDetails {
                    album: self.title.as_deref().and_then(heron_library::tags::clean),
                    record_label: label
                        .and_then(|info| info.label.as_ref())
                        .and_then(|label| heron_library::tags::clean(&label.name)),
                    catalog_number: label
                        .and_then(|info| info.catalog_number.as_deref())
                        .and_then(heron_library::tags::clean)
                        .map(|number| number.chars().take(255).collect()),
                    isrc: recording
                        .isrcs
                        .iter()
                        .find(|isrc| isrc.len() == 12 && isrc.is_ascii())
                        .cloned(),
                    // The earlier: a hit on a compilation came out before
                    // the album, a recording on a reissue can be a later
                    // master of an album track.
                    first_release_year: match (
                        year(recording.first_release_date.as_deref()),
                        release_year,
                    ) {
                        (Some(recording), Some(release)) => Some(recording.min(release)),
                        (recording, release) => recording.or(release),
                    },
                    genres: genre_names([
                        recording.genres.as_slice(),
                        self.genres.as_slice(),
                        group.map_or(empty, |group| group.genres.as_slice()),
                    ]),
                };
                (recording.id.clone(), details)
            })
            .collect()
    }
}

// MusicBrainz artist lookups (`inc=artist-rels`).
#[derive(Debug, Deserialize)]
struct MbArtistRelations {
    #[serde(default)]
    relations: Vec<MbRelation>,
}

#[derive(Debug, Deserialize)]
struct MbRelation {
    #[serde(rename = "type")]
    kind: String,
    /// `forward`: this artist is the subject (`A member of band B`).
    #[serde(default)]
    direction: String,
    artist: Option<MbArtist>,
}

// MusicBrainz artist lookups (`inc=url-rels`).
#[derive(Debug, Deserialize)]
struct MbArtistUrls {
    #[serde(default)]
    relations: Vec<MbUrlRelation>,
}

#[derive(Debug, Deserialize)]
struct MbUrlRelation {
    #[serde(rename = "type")]
    kind: String,
    url: Option<MbUrl>,
}

#[derive(Debug, Deserialize)]
struct MbUrl {
    resource: String,
}

impl MbArtistUrls {
    /// The item of the first `wikidata` link (`https://www.wikidata.org/wiki/Q42`).
    fn wikidata(&self) -> Option<String> {
        self.relations
            .iter()
            .filter(|relation| relation.kind == "wikidata")
            .filter_map(|relation| relation.url.as_ref())
            .find_map(|url| wikidata_item(&url.resource))
    }
}

/// `Q42` from a Wikidata item's address, if it is one.
fn wikidata_item(address: &str) -> Option<String> {
    let item = address.trim_end_matches('/').rsplit('/').next()?;
    let digits = item.strip_prefix('Q')?;
    (!digits.is_empty() && digits.len() <= 15 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| item.to_owned())
}

fn credited(credits: &[MbCredit]) -> Option<String> {
    let name: String = credits
        .iter()
        .map(|credit| format!("{}{}", credit.name, credit.joinphrase))
        .collect();
    heron_library::tags::clean(&name)
}

impl MbRelease {
    /// The tags of `recording` on this release, at the expected position
    /// when it appears more than once.
    fn tags(&self, recording: &str, disc: Option<u32>, track: Option<u32>) -> Option<Tags> {
        let mut places = self.media.iter().flat_map(|medium| {
            medium
                .tracks
                .iter()
                .filter(move |t| t.recording.id == recording)
                .map(move |t| (medium, t))
        });
        let all: Vec<_> = places.by_ref().collect();
        let (medium, found) = all
            .iter()
            .find(|(medium, t)| medium.position == disc && t.position == track)
            .or_else(|| all.first())?;
        let release_group = self.release_group.as_ref();
        let date = release_group
            .and_then(|group| group.first_release_date.clone())
            .filter(|date| !date.is_empty())
            .or_else(|| self.date.clone().filter(|date| !date.is_empty()));
        let album_artist_id = self
            .artist_credit
            .first()
            .map(|credit| credit.artist.id.clone());
        Some(Tags {
            title: heron_library::tags::clean(&found.title),
            artist: credited(&found.artist_credit),
            album_artist: credited(&self.artist_credit),
            album: heron_library::tags::clean(&self.title),
            track: found.position,
            track_total: medium.track_count,
            disc: medium.position,
            disc_total: u32::try_from(self.media.len()).ok(),
            date,
            compilation: album_artist_id.as_deref() == Some(VARIOUS_ARTISTS_ID),
            musicbrainz: MusicBrainzIds {
                recording: Some(found.recording.id.clone()),
                release_track: Some(found.id.clone()),
                release: Some(self.id.clone()),
                release_group: release_group.map(|group| group.id.clone()),
                artist: found
                    .artist_credit
                    .first()
                    .map(|credit| credit.artist.id.clone()),
                album_artist: album_artist_id,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn edition_notes_are_left_out_of_searches() {
        use super::search_title;
        assert_eq!(
            search_title("We Will Rock You (Remastered 2011)"),
            "We Will Rock You"
        );
        assert_eq!(search_title("Drive Home - 2013 Remaster"), "Drive Home");
        assert_eq!(search_title("Jóga [Deluxe Edition] (Live)"), "Jóga (Live)");
        assert_eq!(
            search_title("Bachelorette (Radio Edit)"),
            "Bachelorette (Radio Edit)"
        );
        assert_eq!(search_title("(Remastered)"), "(Remastered)");
    }

    use super::*;

    /// A release in a test search: `(release id, group id, group title,
    /// primary type, secondary types, date, status)`.
    type TestRelease<'a> = (
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        &'a [&'a str],
        &'a str,
        &'a str,
    );

    /// A recording search as MusicBrainz answers it: `recordings`, each
    /// with its ID, title, disambiguation and releases.
    fn found(recordings: &[(&str, &str, &str, &[TestRelease])]) -> MbRecordingSearch {
        let recordings: Vec<serde_json::Value> = recordings
            .iter()
            .map(|(id, title, disambiguation, releases)| {
                serde_json::json!({
                    "id": id, "title": title, "disambiguation": disambiguation, "score": 100,
                    "artist-credit": [{"name": "Queen", "joinphrase": "", "artist": {"id": "queen"}}],
                    "releases": releases.iter().map(|(release, group, name, primary, secondary, date, status)| {
                        serde_json::json!({
                            "id": release, "title": name, "status": status, "date": date,
                            "country": "GB", "track-count": 11,
                            "release-group": {"id": group, "title": name,
                                              "primary-type": primary, "secondary-types": secondary},
                            "media": [{"position": 1, "format": "CD", "track-offset": 0}],
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({ "recordings": recordings })).unwrap()
    }

    fn search_of_a_well_known_song() -> MbRecordingSearch {
        found(&[
            // Covered on one compilation, a lone recording listed first.
            (
                "cover",
                "We Will Rock You",
                "",
                &[(
                    "c1",
                    "g-tribute",
                    "Tribute",
                    "Album",
                    &[],
                    "2000",
                    "Official",
                )],
            ),
            // The original: on its album (three editions), a best-of and a
            // bootleg.
            (
                "original",
                "We Will Rock You",
                "",
                &[
                    (
                        "n3",
                        "g-news",
                        "News of the World",
                        "Album",
                        &[],
                        "2011-03-14",
                        "Official",
                    ),
                    (
                        "n1",
                        "g-news",
                        "News of the World",
                        "Album",
                        &[],
                        "1977-10-28",
                        "Official",
                    ),
                    (
                        "n2",
                        "g-news",
                        "News of the World",
                        "Album",
                        &[],
                        "1977-11-01",
                        "Official",
                    ),
                    (
                        "b1",
                        "g-best",
                        "Greatest Hits",
                        "Album",
                        &["Compilation"],
                        "1981",
                        "Official",
                    ),
                    (
                        "x1",
                        "g-boot",
                        "Live in Rio",
                        "Album",
                        &[],
                        "1985",
                        "Bootleg",
                    ),
                ],
            ),
            // A live recording.
            (
                "live",
                "We Will Rock You",
                "live, 1979",
                &[(
                    "l1",
                    "g-killers",
                    "Live Killers",
                    "Album",
                    &["Live"],
                    "1979",
                    "Official",
                )],
            ),
            // A demo, on the single.
            (
                "demo",
                "We Will Rock You (demo)",
                "",
                &[
                    (
                        "s1",
                        "g-single",
                        "We Will Rock You",
                        "Single",
                        &[],
                        "1977-10-07",
                        "Official",
                    ),
                    (
                        "s2",
                        "g-news",
                        "News of the World",
                        "Album",
                        &[],
                        "2017-11-17",
                        "Official",
                    ),
                ],
            ),
        ])
    }

    #[test]
    fn a_search_gives_one_candidate_per_album_studio_first() {
        let albums = album_candidates(
            search_of_a_well_known_song(),
            "we will rock you",
            None,
            &[AlbumKind::Studio],
            Album::Any,
        );
        let rows: Vec<(&str, &str, &str, Option<u32>)> = albums
            .iter()
            .map(|c| {
                (
                    c.release_group_title.as_deref().unwrap(),
                    c.release_id.as_str(),
                    c.album_date.as_deref().unwrap(),
                    c.editions,
                )
            })
            .collect();
        // The original's earliest edition (not the demo's), every edition
        // counted; no compilation, live album or bootleg.
        assert_eq!(
            rows,
            [
                ("News of the World", "n1", "1977-10-28", Some(4)),
                ("Tribute", "c1", "2000", Some(1)),
            ]
        );
        assert_eq!(albums[0].recording_id, "original");
        assert_eq!(albums[0].album_kind, Some(AlbumKind::Studio));
        assert_eq!(albums[0].format.as_deref(), Some("CD"));
        // Every album has the same score: the order stays.
        assert!(albums.iter().all(|c| (c.score - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn a_search_keeps_the_kinds_asked_and_the_album_asked_first() {
        let albums = album_candidates(
            search_of_a_well_known_song(),
            "we will rock you",
            Some("Live Killers"),
            &[AlbumKind::Studio, AlbumKind::Live, AlbumKind::Single],
            Album::Any,
        );
        let groups: Vec<&str> = albums
            .iter()
            .map(|c| c.release_group_title.as_deref().unwrap())
            .collect();
        assert_eq!(
            groups,
            [
                "Live Killers",
                "News of the World",
                "Tribute",
                "We Will Rock You"
            ]
        );
    }

    #[test]
    fn an_album_asked_by_its_id_is_the_only_one() {
        let only = |asked: AlbumId| {
            album_candidates(
                search_of_a_well_known_song(),
                "we will rock you",
                None,
                &EVERY_KIND,
                Album::Only(&asked),
            )
            .into_iter()
            .map(|c| c.release_id)
            .collect::<Vec<_>>()
        };
        // A compilation too, whatever kinds the search had.
        assert_eq!(only(AlbumId::ReleaseGroup("g-best".into())), ["b1"]);
        // That edition, not the album's earliest.
        assert_eq!(only(AlbumId::Release("n2".into())), ["n2"]);
        assert_eq!(only(AlbumId::Either("n3".into())), ["n3"]);
        assert_eq!(only(AlbumId::Either("g-news".into())), ["n1"]);
        assert!(only(AlbumId::ReleaseGroup("g-other".into())).is_empty());
        // Its tracklist: a candidate per recording on it.
        let asked = AlbumId::ReleaseGroup("g-news".into());
        let tracks = album_candidates(
            search_of_a_well_known_song(),
            "not on it",
            None,
            &EVERY_KIND,
            Album::Tracklist(&asked),
        );
        assert!(!tracks.is_empty());
        assert!(
            tracks
                .iter()
                .all(|c| c.release_group_id.as_deref() == Some("g-news"))
        );
        let mut recordings: Vec<&str> = tracks.iter().map(|c| c.recording_id.as_str()).collect();
        recordings.dedup();
        assert_eq!(recordings.len(), tracks.len());
    }

    #[test]
    fn albums_are_given_by_id_or_address() {
        const ID: &str = "6a1c9b8e-1f0d-4c33-9a39-6d5f0a3b5b11";
        assert_eq!(
            AlbumId::parse(&format!("https://musicbrainz.org/release-group/{ID}")),
            Some(AlbumId::ReleaseGroup(ID.into()))
        );
        assert_eq!(
            AlbumId::parse(&format!(
                " https://musicbrainz.org/release/{}/?tab=x#top ",
                ID.to_uppercase()
            )),
            Some(AlbumId::Release(ID.into()))
        );
        assert_eq!(AlbumId::parse(ID), Some(AlbumId::Either(ID.into())));
        assert_eq!(AlbumId::parse("News of the World"), None);
        assert_eq!(
            AlbumId::parse(&format!("https://musicbrainz.org/artist/{ID}")),
            None
        );
        assert_eq!(AlbumId::parse(&format!("{ID} OR rgid:x")), None);
        assert_eq!(
            AlbumId::Either(ID.into()).query(),
            format!("(rgid:{ID} OR reid:{ID})")
        );
    }

    #[test]
    fn kinds_become_query_clauses() {
        assert_eq!(
            kinds_query(&[AlbumKind::Studio]),
            " AND status:official AND primarytype:(album) AND NOT comment:live"
        );
        assert_eq!(
            kinds_query(&[AlbumKind::Live, AlbumKind::Single]),
            " AND status:official AND primarytype:(album OR single OR ep)"
        );
        assert_eq!(
            kinds_query(&[AlbumKind::Studio, AlbumKind::Other]),
            " AND status:official AND NOT comment:live"
        );
    }

    #[test]
    fn only_refusals_that_will_not_change_are_permanent() {
        let answer = |status| IdentifyError::Service {
            service: "AcoustID",
            status,
            message: String::new(),
        };
        assert!(answer(400).is_permanent());
        assert!(answer(401).is_permanent());
        assert!(!answer(429).is_permanent());
        assert!(!answer(503).is_permanent());
        assert!(!answer(200).is_permanent());
    }

    #[test]
    fn search_phrases_are_quoted() {
        assert_eq!(lucene_phrase(r#"Say "Hi" \ Bye"#), r#""Say \"Hi\" \\ Bye""#);
    }

    #[test]
    fn search_results_are_read() {
        let found: MbReleaseSearch = serde_json::from_str(
            r#"{"created": "x", "count": 1, "offset": 0, "releases": [{
                "id": "b84ee12a-09ef-421b-82de-0441a926375b", "score": 100,
                "title": "Kind of Blue", "date": "1959-08-17", "country": "US",
                "artist-credit": [{"name": "Miles Davis", "joinphrase": "",
                                   "artist": {"id": "561d854a-6a28-4aa7-8c99-323e6ce46c2a"}}]}]}"#,
        )
        .unwrap();
        assert_eq!(found.releases[0].score, 100);
        assert_eq!(credit_text(&found.releases[0].artist_credit), "Miles Davis");
    }

    #[test]
    fn release_details_by_recording() {
        let json = r#"{
            "id": "r1", "title": "Greatest Hits", "date": "2005-03-01",
            "label-info": [
                {"catalog-number": null, "label": null},
                {"catalog-number": "CAT 42", "label": {"name": "Columbia"}}
            ],
            "genres": [{"name": "pop", "count": 2}, {"name": "rock", "count": 5}],
            "release-group": {"first-release-date": "2005", "genres": [{"name": "Rock", "count": 9}]},
            "media": [{"tracks": [
                {"recording": {"id": "rec1", "isrcs": ["USRC17607839"],
                               "first-release-date": "1975-06-01",
                               "genres": [{"name": "disco", "count": 1}]}},
                {"recording": {"id": "rec2"}}
            ]}]
        }"#;
        let release: MbReleaseDetails = serde_json::from_str(json).unwrap();
        let recordings = release.recordings();
        let first = &recordings["rec1"];
        assert_eq!(first.album.as_deref(), Some("Greatest Hits"));
        assert_eq!(first.record_label.as_deref(), Some("Columbia"));
        assert_eq!(first.catalog_number.as_deref(), Some("CAT 42"));
        assert_eq!(first.isrc.as_deref(), Some("USRC17607839"));
        assert_eq!(first.first_release_year, Some(1975));
        assert_eq!(first.genres, ["disco", "rock", "pop"]);
        let second = &recordings["rec2"];
        assert_eq!(second.isrc, None);
        assert_eq!(second.first_release_year, Some(2005));
    }

    /// A real answer (MusicBrainz data is CC0): the 2004 US edition of Kind
    /// of Blue, whose recordings MusicBrainz dates 1959 to 1988.
    #[test]
    fn release_details_of_a_real_release() {
        let release: MbReleaseDetails = serde_json::from_str(include_str!(
            "../../tests/fixtures/musicbrainz-release-kind-of-blue.json"
        ))
        .unwrap();
        let recordings = release.recordings();
        assert_eq!(recordings.len(), 13, "two discs");
        let so_what = &recordings["272dbf93-1501-4650-bbde-627ff0588a73"];
        assert_eq!(so_what.album.as_deref(), Some("Kind of Blue"));
        assert_eq!(so_what.record_label.as_deref(), Some("Columbia"));
        assert_eq!(so_what.catalog_number.as_deref(), Some("CN 90887"));
        assert_eq!(so_what.isrc.as_deref(), Some("USBJN0920070"));
        // The album's 1959, not the 1969 MusicBrainz gives the recording.
        assert_eq!(so_what.first_release_year, Some(1959));
        let alternate = &recordings["bc80349a-c5d6-4d35-8d29-87d5b10e5d5d"];
        assert_eq!(alternate.first_release_year, Some(1959));
    }

    #[test]
    fn an_artist_gets_its_wikidata_item() {
        let artist: MbArtistUrls = serde_json::from_value(serde_json::json!({
            "relations": [
                {"type": "official homepage", "url": {"resource": "https://example.org/"}},
                {"type": "wikidata", "url": {"resource": "https://www.wikidata.org/wiki/Q15862"}},
            ]
        }))
        .unwrap();
        assert_eq!(artist.wikidata().as_deref(), Some("Q15862"));
        let none: MbArtistUrls = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(none.wikidata(), None);
        assert_eq!(
            wikidata_item("https://www.wikidata.org/wiki/Q42/"),
            Some("Q42".into())
        );
        assert_eq!(
            wikidata_item("https://www.wikidata.org/wiki/Property:P31"),
            None
        );
        assert_eq!(wikidata_item("https://www.wikidata.org/wiki/Q"), None);
        assert_eq!(wikidata_item("https://www.wikidata.org/wiki/Q12x"), None);
    }
}
