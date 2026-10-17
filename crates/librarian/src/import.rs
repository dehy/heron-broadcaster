// SPDX-License-Identifier: AGPL-3.0-or-later

//! The incoming folder: files dropped there are identified, tagged and
//! filed into the library, then analyzed like any other track.
//!
//! Files are picked up once they have not changed for the settle time, like
//! the scanner does. Files of one folder are handled together, so siblings
//! agree on their release. When AcoustID or MusicBrainz cannot be reached,
//! the folder waits and is tried again later; after a few failures it is
//! imported with its own tags and flagged for review.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use heron_analysis::Chromaprint;
use heron_core::{FileStamp, LibraryPath, LibrarySettings};
use heron_library::tags::{self, FileTags};
use heron_library::{LibraryLock, Tags, is_audio, is_hidden};
use serde::Serialize;
use sqlx::MySqlPool;
use tokio::sync::{mpsc, watch};

use crate::identify::matching::Candidate;
use crate::identify::{Endpoints, FileToIdentify, Identified, Identifier};
use crate::journal::{
    self, BatchKind, Identification, IdentificationStatus, NewTrack, OpError, Operation, Origin,
    Retag, RetagCover, Roots, Subject,
};
use crate::layout::{FileFacts, Layout};
use crate::{fsops, organize};

/// Lookups of a folder are tried this many times before it is imported
/// unidentified.
const LOOKUP_ATTEMPTS: u32 = 3;
/// Wait before trying a folder again, doubled after each failure.
const RETRY_DELAY: Duration = Duration::from_secs(60);
/// Candidates kept for the review screen.
const MAX_CANDIDATES: usize = 20;
/// Quiet time after the last file system event before looking.
const EVENT_DEBOUNCE: Duration = Duration::from_secs(2);
/// Name of the cover saved in album folders.
const COVER_FILE: &str = "cover.jpg";

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("the incoming directory {path} is not readable: {source}")]
    Root {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Analysis(#[from] heron_analysis::Error),
    #[error("task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// Outcome of one pass over the incoming folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub imported: u64,
    pub identified: u64,
    pub needs_review: u64,
    /// Files still changing, or folders waiting to retry a lookup.
    pub deferred: u64,
    pub failed: u64,
}

struct Retry {
    attempts: u32,
    at: Instant,
}

pub struct Importer {
    pool: MySqlPool,
    roots: Roots,
    endpoints: Endpoints,
    settle: Duration,
    retries: HashMap<String, Retry>,
}

/// An incoming file, read and fingerprinted.
struct Incoming {
    path: LibraryPath,
    stamp: FileStamp,
    read: FileTags,
    chromaprint: Option<Chromaprint>,
}

impl Importer {
    pub fn new(pool: MySqlPool, roots: Roots, endpoints: Endpoints, settle: Duration) -> Self {
        Self {
            pool,
            roots,
            endpoints,
            settle,
            retries: HashMap::new(),
        }
    }

    /// Imports, then looks again after file system events and every
    /// `interval`, until `stopped` turns true.
    pub async fn run(
        mut self,
        interval: Duration,
        watch: bool,
        mut stopped: watch::Receiver<bool>,
    ) {
        let (events_tx, mut events) = mpsc::channel::<()>(1);
        let _watcher = if watch {
            watch_dir(&self.roots.incoming, events_tx)
        } else {
            None
        };
        while !*stopped.borrow() {
            let wait = match self.import_once().await {
                Ok(report) if report.deferred > 0 => self.settle + Duration::from_secs(1),
                Ok(_) => interval,
                Err(error) => {
                    tracing::error!(%error, "import failed");
                    interval
                }
            };
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                Some(()) = events.recv() => {
                    while let Ok(Some(())) = tokio::time::timeout(EVENT_DEBOUNCE, events.recv()).await {}
                }
                _ = stopped.changed() => {}
            }
        }
    }

    /// One pass over the incoming folder.
    pub async fn import_once(&mut self) -> Result<ImportReport, ImportError> {
        let settings = load_settings(&self.pool).await?;
        let layout = Layout::from_settings(&settings);
        let identifier = match &settings.acoustid_key {
            Some(key) => {
                match Identifier::new(self.endpoints.clone(), key.clone(), settings.min_score) {
                    Ok(identifier) => Some(identifier),
                    Err(error) => {
                        tracing::error!(%error, "cannot set up identification");
                        None
                    }
                }
            }
            None => None,
        };

        let root = self.roots.incoming.clone();
        let files = tokio::task::spawn_blocking(move || walk(&root)).await??;
        let mut report = ImportReport::default();
        let settled_before =
            now_us().saturating_sub(i64::try_from(self.settle.as_micros()).unwrap_or(0));
        let mut groups: BTreeMap<String, Vec<(LibraryPath, FileStamp)>> = BTreeMap::new();
        for (path, stamp) in files {
            if stamp.mtime_us > settled_before {
                report.deferred += 1;
                continue;
            }
            let dir = path
                .as_str()
                .rsplit_once('/')
                .map_or("", |(dir, _)| dir)
                .to_owned();
            groups.entry(dir).or_default().push((path, stamp));
        }

        let mut batch = None;
        for (dir, files) in groups {
            if self
                .retries
                .get(&dir)
                .is_some_and(|retry| retry.at > Instant::now())
            {
                report.deferred += files.len() as u64;
                continue;
            }
            let incoming = self.read_group(files, identifier.is_some()).await?;
            let identified = match &identifier {
                None => vec![None; incoming.len()],
                Some(identifier) => match identify(identifier, &incoming).await {
                    Ok(identified) => {
                        self.retries.remove(&dir);
                        identified.into_iter().map(Some).collect()
                    }
                    Err(error) => {
                        let retry = self.retries.entry(dir.clone()).or_insert(Retry {
                            attempts: 0,
                            at: Instant::now(),
                        });
                        retry.attempts += 1;
                        if retry.attempts < LOOKUP_ATTEMPTS {
                            retry.at = Instant::now() + RETRY_DELAY * 2_u32.pow(retry.attempts - 1);
                            tracing::warn!(dir, %error, attempt = retry.attempts, "identification failed, will retry");
                            report.deferred += incoming.len() as u64;
                            continue;
                        }
                        tracing::warn!(dir, %error, "identification keeps failing, importing as is");
                        self.retries.remove(&dir);
                        let reason = format!("identification failed: {error}");
                        vec![
                            Some(Identified::Review {
                                reason,
                                candidates: Vec::new()
                            });
                            incoming.len()
                        ]
                    }
                },
            };
            let covers = match &identifier {
                Some(identifier) if settings.save_cover || settings.embed_cover => {
                    fetch_covers(identifier, &identified).await
                }
                _ => HashMap::new(),
            };

            let batch_id = match batch {
                Some(id) => id,
                None => {
                    *batch.insert(journal::create_batch(&self.pool, BatchKind::Import, None).await?)
                }
            };
            let lock = LibraryLock::acquire(&self.pool, "librarian").await?;
            let result = self
                .file_group(
                    batch_id,
                    &layout,
                    &settings,
                    incoming,
                    identified,
                    &covers,
                    &mut report,
                )
                .await;
            lock.release().await?;
            result?;
            fsops::remove_empty_dirs(
                &LibraryPath::from_stored(dir).resolve(&self.roots.incoming),
                &self.roots.incoming,
            );
        }

        if let Some(batch_id) = batch {
            journal::finish_batch(&self.pool, batch_id).await?;
        }
        if report.imported > 0 {
            let analysis = heron_analysis::jobs::load_settings(&self.pool).await?;
            heron_analysis::jobs::enqueue_stale(&self.pool, &analysis).await?;
            tracing::info!(?report, "incoming files imported");
        }
        for (result, count) in [
            ("identified", report.identified),
            ("needs_review", report.needs_review),
            (
                "unidentified",
                report.imported - report.identified - report.needs_review,
            ),
            ("failed", report.failed),
        ] {
            metrics::counter!("heron_librarian_imports_total", "result" => result).increment(count);
        }
        Ok(report)
    }

    async fn read_group(
        &self,
        files: Vec<(LibraryPath, FileStamp)>,
        fingerprint: bool,
    ) -> Result<Vec<Incoming>, ImportError> {
        let root = self.roots.incoming.clone();
        Ok(tokio::task::spawn_blocking(move || {
            files
                .into_iter()
                .map(|(path, stamp)| {
                    let absolute = path.resolve(&root);
                    let read = tags::read(&absolute).unwrap_or_else(|error| {
                        tracing::warn!(%path, %error, "cannot read tags");
                        FileTags::default()
                    });
                    let chromaprint = if fingerprint {
                        heron_analysis::fingerprint_file(&absolute).unwrap_or_else(|error| {
                            tracing::warn!(%path, %error, "cannot fingerprint");
                            None
                        })
                    } else {
                        None
                    };
                    Incoming {
                        path,
                        stamp,
                        read,
                        chromaprint,
                    }
                })
                .collect()
        })
        .await?)
    }

    /// Files a group into the library. Hold the library lock.
    #[allow(clippy::too_many_arguments)]
    async fn file_group(
        &self,
        batch_id: u64,
        layout: &Layout,
        settings: &LibrarySettings,
        incoming: Vec<Incoming>,
        identified: Vec<Option<Identified>>,
        covers: &HashMap<String, Vec<u8>>,
        report: &mut ImportReport,
    ) -> Result<(), ImportError> {
        let mut planned = HashSet::new();
        let mut album_dirs: HashMap<String, PathBuf> = HashMap::new();
        for (file, identified) in incoming.into_iter().zip(identified) {
            let own = file.read.tags.clone();
            let (tags, identification, release) = match identified {
                None => (own.clone(), None, None),
                Some(Identified::Match {
                    tags,
                    score,
                    recording_mbid,
                    release_mbid,
                    candidates,
                }) => (
                    merge(*tags, &own),
                    Some(Identification {
                        status: IdentificationStatus::Identified,
                        reason: None,
                        score: Some(score),
                        recording_mbid: Some(recording_mbid),
                        release_mbid: Some(release_mbid.clone()),
                        candidates: candidates_json(&candidates),
                    }),
                    Some(release_mbid),
                ),
                Some(Identified::Review { reason, candidates }) => (
                    own.clone(),
                    Some(Identification {
                        status: IdentificationStatus::NeedsReview,
                        reason: Some(reason),
                        score: candidates.iter().map(|c| c.score).reduce(f64::max),
                        recording_mbid: None,
                        release_mbid: None,
                        candidates: candidates_json(&candidates),
                    }),
                    None,
                ),
            };

            let cover = release
                .as_ref()
                .and_then(|release| covers.get(release))
                .filter(|_| settings.embed_cover && !file.read.has_cover);
            let retag = (tags != own || cover.is_some()).then(|| Retag {
                previous: own.clone(),
                applied: tags.clone(),
                cover: cover.map_or(RetagCover::Keep, |jpeg| RetagCover::Embed(jpeg.clone())),
            });

            let absolute = file.path.resolve(&self.roots.incoming);
            let (stem, extension) = organize::stem_and_extension(&absolute);
            let desired = layout.render(&FileFacts {
                tags: &tags,
                original_stem: &stem,
                extension: &extension,
            });
            let to = loop {
                let candidate = organize::free_path(&desired, None, &mut planned, |path| {
                    std::fs::symlink_metadata(self.roots.library.join(path)).is_ok()
                });
                let Some(candidate) = candidate.map(LibraryPath::from_stored) else {
                    continue;
                };
                if journal::is_free(&self.pool, &self.roots, &candidate).await? {
                    break candidate;
                }
            };

            let status = identification
                .as_ref()
                .map(|identification| identification.status);
            let operation = Operation {
                batch_id,
                subject: Subject::New(Box::new(NewTrack {
                    tags: tags.clone(),
                    duration_ms: file.read.duration_ms,
                    chromaprint: file.chromaprint,
                    identification,
                })),
                from: Origin::Incoming(file.path.clone()),
                to: to.clone(),
                retag,
                expected_stamp: Some(file.stamp),
            };
            match journal::execute(&self.pool, &self.roots, operation).await {
                Ok(_) => {
                    report.imported += 1;
                    match status {
                        Some(IdentificationStatus::Identified) => report.identified += 1,
                        Some(IdentificationStatus::NeedsReview) => report.needs_review += 1,
                        None => {}
                    }
                    tracing::info!(from = %file.path, %to, ?status, "imported");
                    if let (Some(release), Some(parent)) =
                        (release, to.resolve(&self.roots.library).parent())
                    {
                        album_dirs
                            .entry(release)
                            .or_insert_with(|| parent.to_path_buf());
                    }
                }
                Err(OpError::Database(error)) => return Err(error.into()),
                Err(OpError::SourceChanged) => report.deferred += 1,
                Err(error) => {
                    tracing::warn!(path = %file.path, %error, "cannot import");
                    report.failed += 1;
                }
            }
        }

        if settings.save_cover {
            for (release, dir) in album_dirs {
                if let Some(jpeg) = covers.get(&release) {
                    save_cover(&dir, jpeg);
                }
            }
        }
        Ok(())
    }
}

/// Identifies a group.
async fn identify(
    identifier: &Identifier,
    incoming: &[Incoming],
) -> Result<Vec<Identified>, crate::identify::IdentifyError> {
    let files: Vec<FileToIdentify> = incoming
        .iter()
        .map(|file| FileToIdentify {
            chromaprint: file.chromaprint.clone(),
            duration_s: file.read.duration_ms.map(|ms| (ms + 500) / 1000),
            album_tag: file.read.tags.album.clone(),
        })
        .collect();
    identifier.identify(&files).await
}

async fn fetch_covers(
    identifier: &Identifier,
    identified: &[Option<Identified>],
) -> HashMap<String, Vec<u8>> {
    let mut covers = HashMap::new();
    let releases: HashSet<&String> = identified
        .iter()
        .filter_map(|identified| match identified {
            Some(Identified::Match { release_mbid, .. }) => Some(release_mbid),
            _ => None,
        })
        .collect();
    for release in releases {
        match identifier.cover(release).await {
            Ok(Some(jpeg)) => {
                covers.insert(release.clone(), jpeg);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(release, %error, "cannot fetch the cover"),
        }
    }
    covers
}

/// Saves `cover.jpg` in an album folder, unless there is one already.
pub(crate) fn save_cover(dir: &Path, jpeg: &[u8]) {
    let target = dir.join(COVER_FILE);
    if target.exists() {
        return;
    }
    let temp = dir.join(format!(".heron-{COVER_FILE}"));
    let written = std::fs::write(&temp, jpeg)
        .and_then(|()| fsops::sync_file(&temp))
        .and_then(|()| fsops::rename_no_replace(&temp, &target));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        tracing::warn!(dir = %dir.display(), %error, "cannot save the cover");
    }
}

/// The identified tags, keeping the file's own values for fields
/// MusicBrainz left empty.
pub(crate) fn merge(identified: Tags, own: &Tags) -> Tags {
    Tags {
        title: identified.title.or_else(|| own.title.clone()),
        artist: identified.artist.or_else(|| own.artist.clone()),
        album_artist: identified.album_artist.or_else(|| own.album_artist.clone()),
        album: identified.album.or_else(|| own.album.clone()),
        track: identified.track.or(own.track),
        track_total: identified.track_total.or(own.track_total),
        disc: identified.disc.or(own.disc),
        disc_total: identified.disc_total.or(own.disc_total),
        date: identified.date.or_else(|| own.date.clone()),
        compilation: identified.compilation,
        musicbrainz: identified.musicbrainz,
    }
}

fn candidates_json(candidates: &[Candidate]) -> Option<serde_json::Value> {
    if candidates.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Candidate> = candidates.iter().collect();
    sorted.sort_by(|a, b| b.score.total_cmp(&a.score));
    serde_json::to_value(sorted.into_iter().take(MAX_CANDIDATES).collect::<Vec<_>>()).ok()
}

/// Reads the library settings from the `settings` table, falling back to
/// the defaults when the row is missing or invalid.
pub async fn load_settings(pool: &MySqlPool) -> Result<LibrarySettings, sqlx::Error> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT CAST(value AS CHAR) FROM settings WHERE name = ?")
            .bind(LibrarySettings::SETTINGS_NAME)
            .fetch_optional(pool)
            .await?;
    let Some(value) = value else {
        return Ok(LibrarySettings::default());
    };
    match serde_json::from_str(&value) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            tracing::error!(%error, "invalid library settings, using the defaults");
            Ok(LibrarySettings::default())
        }
    }
}

/// Audio files in the incoming folder, with their stamps.
fn walk(root: &Path) -> Result<Vec<(LibraryPath, FileStamp)>, ImportError> {
    let root_error = |source| ImportError::Root {
        path: root.to_path_buf(),
        source,
    };
    if !std::fs::metadata(root).map_err(root_error)?.is_dir() {
        return Err(root_error(std::io::Error::other("not a directory")));
    }
    let mut files = Vec::new();
    let entries = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !is_hidden(entry.file_name()));
    for entry in entries.flatten() {
        if !entry.file_type().is_file() || !is_audio(entry.path()) {
            continue;
        }
        let Some(path) = LibraryPath::from_file(root, entry.path()) else {
            tracing::warn!(path = %entry.path().display(), "skipping file with a non UTF-8 name");
            continue;
        };
        if let Ok(metadata) = entry.metadata() {
            files.push((path, FileStamp::from_metadata(&metadata)));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn watch_dir(dir: &Path, events: mpsc::Sender<()>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let handler = move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = events.try_send(());
        }
    };
    let watcher = notify::recommended_watcher(handler).and_then(|mut watcher| {
        watcher.watch(dir, notify::RecursiveMode::Recursive)?;
        Ok(watcher)
    });
    match watcher {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::warn!(%error, "cannot watch the incoming folder, relying on periodic checks");
            None
        }
    }
}

fn now_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_micros()).unwrap_or(i64::MAX)
        })
}
