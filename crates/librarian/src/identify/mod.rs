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

use std::collections::HashMap;
use std::time::Duration;

use heron_analysis::Chromaprint;
use heron_library::{MusicBrainzIds, Tags};
use matching::{Candidate, Choice, FileMatches};
use ratelimit::RateLimiter;
use reqwest::StatusCode;
use serde::Deserialize;

/// MusicBrainz ID of the "Various Artists" artist.
const VARIOUS_ARTISTS_ID: &str = "89ad4ac3-39f7-470e-963a-56509c546377";

/// Attempts at a request the service answered with 503 (slow down) or 429.
const ATTEMPTS: u32 = 3;

/// Base URLs, configurable for mirrors and tests.
#[derive(Clone, Debug)]
pub struct Endpoints {
    /// The AcoustID lookup URL.
    pub acoustid: String,
    /// The MusicBrainz web service root, ending with `/ws/2`.
    pub musicbrainz: String,
    pub cover_art: String,
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
    acoustid: RateLimiter,
    musicbrainz: RateLimiter,
    cover_art: RateLimiter,
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
            endpoints,
            key,
            min_score,
            acoustid: RateLimiter::new(Duration::from_millis(334)),
            musicbrainz: RateLimiter::new(Duration::from_secs(1)),
            cover_art: RateLimiter::new(Duration::from_secs(1)),
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
                    .back_off(Duration::from_secs(2_u64.pow(attempt)))
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
    #[serde(default)]
    releasegroups: Vec<AcoustidReleaseGroup>,
}

#[derive(Debug, Deserialize)]
struct AcoustidReleaseGroup {
    id: String,
    title: Option<String>,
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
