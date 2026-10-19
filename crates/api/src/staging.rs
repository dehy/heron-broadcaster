// SPDX-License-Identifier: AGPL-3.0-or-later

//! The buffer zone: uploads, and the files waiting there to be imported.
//!
//! Uploads are written into the buffer folder (`HERON_STAGING_DIR`), never
//! into the library. A file is created empty, then sent in chunks of at
//! most [`MAX_CHUNK`] bytes, each at the offset the server has: after a
//! dropped connection the client asks for the file's `received` bytes and
//! carries on from there. The file is written under a hidden temporary name
//! and shows in the buffer only once complete.
//!
//! heron-librarian identifies complete files and imports them when asked
//! (a library task); picking another release or discarding a file is done
//! here, in the database and the buffer folder.

use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use heron_core::library::{
    StagedChoice, StagedFlag, StagedIdentification, StagedIdentificationStatus, TaskRequest,
    upload_part_name,
};
use heron_core::{LibraryPath, LibrarySettings};
use heron_library::{Tags, is_audio, layout};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody};
use crate::librarian::{LibraryTask, ReviewCandidate, ask};
use crate::settings;

/// Largest chunk of an upload, in bytes. `heron-web` lets bodies this big
/// through to the API.
pub const MAX_CHUNK: usize = 16 * 1024 * 1024;
/// Files imported or discarded in one request.
const MAX_FILES: usize = 5_000;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_buffer))
        .routes(routes!(create_group))
        .routes(routes!(create_upload))
        .routes(routes!(get_upload, send_chunk).layer(DefaultBodyLimit::max(MAX_CHUNK)))
        .routes(routes!(choose))
        .routes(routes!(get_staged_audio))
        .routes(routes!(import_files))
        .routes(routes!(discard_files))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StagedStatus {
    /// Being uploaded (or moved in from the incoming folder).
    Receiving,
    /// Complete, waiting for heron-librarian.
    Identifying,
    /// Identified and checked: can be imported.
    Ready,
    Importing,
    Imported,
    Discarded,
    Failed,
}

impl StagedStatus {
    fn parse(status: &str) -> Self {
        match status {
            "receiving" => Self::Receiving,
            "identifying" => Self::Identifying,
            "ready" => Self::Ready,
            "importing" => Self::Importing,
            "imported" => Self::Imported,
            "discarded" => Self::Discarded,
            _ => Self::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupSource {
    Upload,
    /// A folder dropped into the incoming folder.
    Incoming,
}

/// The tags of a file, as read or as they would be written.
#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct StagedTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<u32>,
    pub track_total: Option<u32>,
    pub disc: Option<u32>,
    pub disc_total: Option<u32>,
    pub date: Option<String>,
    pub compilation: bool,
}

impl From<Tags> for StagedTags {
    fn from(tags: Tags) -> Self {
        Self {
            title: tags.title,
            artist: tags.artist,
            album_artist: tags.album_artist,
            album: tags.album,
            track: tags.track,
            track_total: tags.track_total,
            disc: tags.disc,
            disc_total: tags.disc_total,
            date: tags.date,
            compilation: tags.compilation,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct StagedIdentificationView {
    pub status: StagedIdentificationStatus,
    /// Why it needs review.
    pub reason: Option<String>,
    pub score: Option<f64>,
    pub recording_id: Option<String>,
    pub release_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct StagedFile {
    pub id: u64,
    pub group_id: u64,
    /// Its path in the upload, or in the incoming folder.
    pub name: String,
    pub size: u64,
    /// Bytes received so far, while it is uploaded.
    pub received: Option<u64>,
    pub status: StagedStatus,
    /// Why it should be looked at before it is imported.
    pub flags: Vec<StagedFlag>,
    /// The file's own tags.
    pub own_tags: Option<StagedTags>,
    /// The tags it gets when imported.
    pub tags: Option<StagedTags>,
    pub duration_ms: Option<u32>,
    /// Where the layout would file it in the library today.
    pub planned_path: Option<String>,
    pub identification: Option<StagedIdentificationView>,
    /// Releases it may be on, best first.
    pub candidates: Vec<ReviewCandidate>,
    /// Picked by hand: another release, or its own tags.
    pub choice: Option<StagedChoice>,
    /// The release it is imported as (its cover is on the Cover Art Archive).
    pub release_id: Option<String>,
    pub duplicate_of_track_id: Option<u64>,
    pub duplicate_of_file_id: Option<u64>,
    /// The track it became.
    pub track_id: Option<u64>,
    pub error: Option<String>,
    pub uploaded_by: Option<String>,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct StagedGroup {
    pub id: u64,
    pub source: GroupSource,
    pub name: String,
    pub created_by: Option<String>,
    pub created_at_unix_ms: i64,
    pub files: Vec<StagedFile>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Buffer {
    /// Groups with files still to import or discard, newest first.
    pub groups: Vec<StagedGroup>,
    pub max_upload_bytes: u64,
    /// Free space on the buffer folder's disk.
    pub free_bytes: Option<u64>,
    /// Uploads are refused below this much free space.
    pub min_free_bytes: u64,
}

const FILE_COLUMNS: &str = "f.id, f.group_id, f.staged_path, f.original_name, f.size,
    CAST(f.status AS CHAR) AS status, CAST(f.flags AS CHAR) AS flags,
    CAST(f.tags AS CHAR) AS tags, CAST(f.applied_tags AS CHAR) AS applied_tags,
    f.duration_ms, f.planned_path, CAST(f.identification AS CHAR) AS identification,
    CAST(f.choice AS CHAR) AS choice, f.duplicate_of_track_id, f.duplicate_of_staged_id,
    f.track_id, f.error, u.username,
    CAST(UNIX_TIMESTAMP(f.created_at) * 1000 AS SIGNED) AS created_ms
    FROM staged_files f LEFT JOIN users u ON u.id = f.uploaded_by";

fn parse<T: serde::de::DeserializeOwned>(json: Option<String>) -> Option<T> {
    json.and_then(|json| serde_json::from_str(&json).ok())
}

fn staged_file(row: &sqlx::mysql::MySqlRow, staging: &std::path::Path) -> ApiResult<StagedFile> {
    let id: u64 = row.try_get("id")?;
    let group_id: u64 = row.try_get("group_id")?;
    let status = StagedStatus::parse(&row.try_get::<String, _>("status")?);
    let identification: Option<StagedIdentification> = parse(row.try_get("identification")?);
    let choice: Option<StagedChoice> = parse(row.try_get("choice")?);
    let received = (status == StagedStatus::Receiving).then(|| {
        std::fs::metadata(
            staging
                .join(group_id.to_string())
                .join(upload_part_name(id)),
        )
        .map_or(0, |metadata| metadata.len())
    });
    let release_id = match (&choice, &identification) {
        (Some(StagedChoice::Release { release_id, .. }), _) => Some(release_id.clone()),
        (None, Some(identification))
            if identification.status == StagedIdentificationStatus::Identified =>
        {
            identification.release_mbid.clone()
        }
        _ => None,
    };
    Ok(StagedFile {
        id,
        group_id,
        name: row.try_get("original_name")?,
        size: row.try_get("size")?,
        received,
        status,
        flags: parse(row.try_get("flags")?).unwrap_or_default(),
        own_tags: parse::<Tags>(row.try_get("tags")?).map(StagedTags::from),
        tags: parse::<Tags>(row.try_get("applied_tags")?).map(StagedTags::from),
        duration_ms: row.try_get("duration_ms")?,
        planned_path: row.try_get("planned_path")?,
        candidates: identification
            .as_ref()
            .map(|identification| {
                identification
                    .candidates
                    .iter()
                    .filter_map(|candidate| serde_json::from_value(candidate.clone()).ok())
                    .collect()
            })
            .unwrap_or_default(),
        identification: identification.map(|identification| StagedIdentificationView {
            status: identification.status,
            reason: identification.reason,
            score: identification.score,
            recording_id: identification.recording_mbid,
            release_id: identification.release_mbid,
        }),
        choice,
        release_id,
        duplicate_of_track_id: row.try_get("duplicate_of_track_id")?,
        duplicate_of_file_id: row.try_get("duplicate_of_staged_id")?,
        track_id: row.try_get("track_id")?,
        error: row.try_get("error")?,
        uploaded_by: row.try_get("username")?,
        created_at_unix_ms: row.try_get("created_ms")?,
    })
}

async fn find_file(state: &AppState, id: u64) -> ApiResult<StagedFile> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {FILE_COLUMNS} WHERE f.id = ?"
    )))
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("file"))?;
    staged_file(&row, &state.staging)
}

/// Free space on the buffer folder's disk.
fn free_bytes(staging: &std::path::Path) -> Option<u64> {
    let stats = rustix::fs::statvfs(staging).ok()?;
    Some(stats.f_bavail.saturating_mul(stats.f_frsize))
}

const MIB: u64 = 1024 * 1024;

/// The buffer zone: groups with files to import or discard, with their
/// identification, flags and planned paths.
#[utoipa::path(get, path = "/staging", tag = "buffer",
    responses((status = 200, body = Buffer), (status = "4XX", body = ErrorBody)))]
async fn get_buffer(_user: CurrentUser, State(state): State<AppState>) -> ApiResult<Json<Buffer>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {FILE_COLUMNS}
         WHERE f.status IN ('receiving', 'identifying', 'ready', 'importing', 'failed')
         ORDER BY f.group_id DESC, f.original_name, f.id"
    )))
    .fetch_all(&state.pool)
    .await?;
    let mut files: HashMap<u64, Vec<StagedFile>> = HashMap::new();
    for row in &rows {
        let file = staged_file(row, &state.staging)?;
        files.entry(file.group_id).or_default().push(file);
    }
    let mut groups = Vec::with_capacity(files.len());
    if !files.is_empty() {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT g.id, CAST(g.source AS CHAR), g.name, u.username,
                    CAST(UNIX_TIMESTAMP(g.created_at) * 1000 AS SIGNED)
             FROM staged_groups g LEFT JOIN users u ON u.id = g.created_by WHERE g.id IN (",
        );
        let mut separated = query.separated(", ");
        for id in files.keys() {
            separated.push_bind(*id);
        }
        query.push(") ORDER BY g.id DESC");
        let rows: Vec<(u64, String, String, Option<String>, i64)> =
            query.build_query_as().fetch_all(&state.pool).await?;
        for (id, source, name, created_by, created) in rows {
            groups.push(StagedGroup {
                id,
                source: if source == "incoming" {
                    GroupSource::Incoming
                } else {
                    GroupSource::Upload
                },
                name,
                created_by,
                created_at_unix_ms: created,
                files: files.remove(&id).unwrap_or_default(),
            });
        }
    }
    let settings: LibrarySettings =
        settings::load(&state.pool, LibrarySettings::SETTINGS_NAME).await?;
    Ok(Json(Buffer {
        groups,
        max_upload_bytes: settings.max_upload_mb.saturating_mul(MIB),
        free_bytes: free_bytes(&state.staging),
        min_free_bytes: settings.min_free_mb.saturating_mul(MIB),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewGroup {
    /// What the files are, for people (`Album 2024`, the dropped folder).
    pub name: String,
}

/// Starts an upload: its files are identified together, so the files of an
/// album agree on their release. Operators and admins.
#[utoipa::path(post, path = "/staging/groups", tag = "buffer",
    request_body = NewGroup,
    responses((status = 201, body = StagedGroup), (status = "4XX", body = ErrorBody)))]
async fn create_group(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(group): Json<NewGroup>,
) -> ApiResult<(StatusCode, Json<StagedGroup>)> {
    user.require(Role::Operator)?;
    let name: String = group.name.trim().chars().take(512).collect();
    let name = if name.is_empty() {
        "Upload".to_owned()
    } else {
        name
    };
    let id =
        sqlx::query("INSERT INTO staged_groups (source, name, created_by) VALUES ('upload', ?, ?)")
            .bind(&name)
            .bind(user.id)
            .execute(&state.pool)
            .await?
            .last_insert_id();
    let created: i64 = sqlx::query_scalar(
        "SELECT CAST(UNIX_TIMESTAMP(created_at) * 1000 AS SIGNED) FROM staged_groups WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(StagedGroup {
            id,
            source: GroupSource::Upload,
            name,
            created_by: Some(user.username),
            created_at_unix_ms: created,
            files: Vec::new(),
        }),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewUpload {
    pub group_id: u64,
    /// The file's path in what was dropped (`Album/01 Song.flac`).
    pub name: String,
    /// Size in bytes.
    pub size: u64,
}

/// A safe relative path for an uploaded file: each part made safe on every
/// file system, hidden parts made visible. `None` without a file name.
fn safe_name(name: &str) -> Option<String> {
    let parts: Vec<String> = name
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .map(layout::safe_component)
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Announces a file of an upload: it is created empty, and its content is
/// sent in chunks. Refused when it is not audio, when it is larger than
/// `max_upload_mb`, or when it would leave less than `min_free_mb` on the
/// disk. Operators and admins.
#[utoipa::path(post, path = "/staging/uploads", tag = "buffer",
    request_body = NewUpload,
    responses((status = 201, body = StagedFile), (status = "4XX", body = ErrorBody)))]
async fn create_upload(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(upload): Json<NewUpload>,
) -> ApiResult<(StatusCode, Json<StagedFile>)> {
    user.require(Role::Operator)?;
    let source: Option<String> =
        sqlx::query_scalar("SELECT CAST(source AS CHAR) FROM staged_groups WHERE id = ?")
            .bind(upload.group_id)
            .fetch_optional(&state.pool)
            .await?;
    match source.as_deref() {
        None => return Err(ApiError::not_found("group")),
        Some("upload") => {}
        Some(_) => {
            return Err(ApiError::Invalid(
                "upload into a group of uploads".to_owned(),
            ));
        }
    }
    let name = safe_name(&upload.name)
        .ok_or_else(|| ApiError::Invalid("the file has no name".to_owned()))?;
    if !is_audio(std::path::Path::new(&name)) {
        return Err(ApiError::Invalid(format!(
            "{name}: not an audio file Heron imports ({})",
            heron_library::files::AUDIO_EXTENSIONS.join(", ")
        )));
    }
    let settings: LibrarySettings =
        settings::load(&state.pool, LibrarySettings::SETTINGS_NAME).await?;
    let max = settings.max_upload_mb.saturating_mul(MIB);
    if upload.size > max {
        return Err(ApiError::Invalid(format!(
            "{name} is larger than the upload limit ({} MiB)",
            settings.max_upload_mb
        )));
    }
    let group_dir = state.staging.join(upload.group_id.to_string());
    std::fs::create_dir_all(&group_dir)
        .map_err(|error| ApiError::Internal(format!("cannot write to the buffer zone: {error}")))?;
    // Bytes still to come for the uploads in progress count as used.
    let pending: Option<i64> = sqlx::query_scalar(
        "SELECT CAST(SUM(size) AS SIGNED) FROM staged_files WHERE status = 'receiving'",
    )
    .fetch_one(&state.pool)
    .await?;
    let pending = u64::try_from(pending.unwrap_or(0)).unwrap_or(0);
    if let Some(free) = free_bytes(&state.staging)
        && free.saturating_sub(pending).saturating_sub(upload.size)
            < settings.min_free_mb.saturating_mul(MIB)
    {
        return Err(ApiError::Invalid(format!(
            "not enough free space in the buffer zone: {} MiB free, {} MiB must stay free",
            free / MIB,
            settings.min_free_mb
        )));
    }

    let staged_path = free_staged_path(&state, upload.group_id, &name).await?;
    let id = sqlx::query(
        "INSERT INTO staged_files (group_id, staged_path, original_name, size, status, uploaded_by)
         VALUES (?, ?, ?, ?, 'receiving', ?)",
    )
    .bind(upload.group_id)
    .bind(&staged_path)
    .bind(upload.name.chars().take(4096).collect::<String>())
    .bind(upload.size)
    .bind(user.id)
    .execute(&state.pool)
    .await?
    .last_insert_id();
    let part = group_dir.join(upload_part_name(id));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .map_err(|error| {
            ApiError::Internal(format!("cannot create {}: {error}", part.display()))
        })?;
    if upload.size == 0 {
        complete(&state, id).await?;
    }
    Ok((StatusCode::CREATED, Json(find_file(&state, id).await?)))
}

/// A path in the group's folder that no other file has, adding ` (2)`, ` (3)`, …
async fn free_staged_path(state: &AppState, group_id: u64, name: &str) -> ApiResult<String> {
    let taken: HashSet<String> = sqlx::query_scalar(
        "SELECT LOWER(staged_path) FROM staged_files
         WHERE group_id = ? AND status NOT IN ('discarded', 'imported')",
    )
    .bind(group_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .collect();
    let desired = format!("{group_id}/{name}");
    for suffix in 1.. {
        let candidate = if suffix == 1 {
            desired.clone()
        } else {
            layout::with_suffix(&desired, suffix)
        };
        let on_disk = LibraryPath::from_stored(candidate.clone()).resolve(&state.staging);
        if !taken.contains(&candidate.to_lowercase()) && std::fs::symlink_metadata(on_disk).is_err()
        {
            return Ok(candidate);
        }
    }
    Err(ApiError::Internal("no free name".to_owned()))
}

/// A file of an upload, with the bytes received so far: send the rest from
/// there.
#[utoipa::path(get, path = "/staging/uploads/{id}", tag = "buffer",
    params(("id" = u64, Path)),
    responses((status = 200, body = StagedFile), (status = "4XX", body = ErrorBody)))]
async fn get_upload(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<StagedFile>> {
    Ok(Json(find_file(&state, id).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ChunkQuery {
    /// Where the chunk goes in the file: the bytes received so far.
    pub offset: u64,
}

/// Files a chunk is being written to: one request at a time per file.
static WRITING: Mutex<Option<HashSet<u64>>> = Mutex::new(None);

struct Writing(u64);

impl Writing {
    fn claim(id: u64) -> Option<Self> {
        let mut writing = WRITING.lock().ok()?;
        writing
            .get_or_insert_with(HashSet::new)
            .insert(id)
            .then_some(Self(id))
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        if let Ok(mut writing) = WRITING.lock()
            && let Some(writing) = writing.as_mut()
        {
            writing.remove(&self.0);
        }
    }
}

/// Appends a chunk (at most 16 MiB) to a file being uploaded, at `offset`,
/// which must be the bytes received so far (409 otherwise: ask for the
/// file and carry on from its `received`). With its last byte, the file is
/// complete and heron-librarian identifies it. Operators and admins.
#[utoipa::path(put, path = "/staging/uploads/{id}", tag = "buffer",
    params(("id" = u64, Path), ChunkQuery),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 200, body = StagedFile), (status = "4XX", body = ErrorBody)))]
async fn send_chunk(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Query(query): Query<ChunkQuery>,
    chunk: Bytes,
) -> ApiResult<Json<StagedFile>> {
    user.require(Role::Operator)?;
    let Some(_writing) = Writing::claim(id) else {
        return Err(ApiError::Conflict(
            "another chunk of this file is being written".to_owned(),
        ));
    };
    let row: Option<(u64, u64, String)> = sqlx::query_as(
        "SELECT group_id, size, CAST(status AS CHAR) FROM staged_files WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let (group_id, size, status) = row.ok_or_else(|| ApiError::not_found("file"))?;
    if status != "receiving" {
        return Err(ApiError::Conflict(format!(
            "the file is {status}, not being uploaded"
        )));
    }
    let part = state
        .staging
        .join(group_id.to_string())
        .join(upload_part_name(id));
    let end = query.offset.saturating_add(chunk.len() as u64);
    if end > size {
        return Err(ApiError::Invalid(format!(
            "the chunk ends at byte {end}, after the end of the file ({size})"
        )));
    }
    let written = tokio::task::spawn_blocking(move || -> std::io::Result<Option<u64>> {
        let mut file = std::fs::OpenOptions::new().write(true).open(&part)?;
        let received = file.metadata()?.len();
        if received != query.offset {
            return Ok(Some(received));
        }
        file.seek(SeekFrom::Start(received))?;
        file.write_all(&chunk)?;
        if end == size {
            file.sync_all()?;
        }
        Ok(None)
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;
    match written {
        Ok(None) => {}
        Ok(Some(received)) => {
            return Err(ApiError::Conflict(format!(
                "{received} bytes received so far: send from there"
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(ApiError::Conflict("the upload was discarded".to_owned()));
        }
        Err(error) => {
            return Err(ApiError::Internal(format!(
                "cannot write the upload: {error}"
            )));
        }
    }
    // Progress keeps the librarian from identifying the group too early.
    sqlx::query("UPDATE staged_files SET updated_at = NOW(6) WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if end == size {
        complete(&state, id).await?;
    }
    Ok(Json(find_file(&state, id).await?))
}

/// The upload is complete: the file takes its name and waits for the
/// librarian.
async fn complete(state: &AppState, id: u64) -> ApiResult<()> {
    let (group_id, staged_path): (u64, String) =
        sqlx::query_as("SELECT group_id, staged_path FROM staged_files WHERE id = ?")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    let part = state
        .staging
        .join(group_id.to_string())
        .join(upload_part_name(id));
    let mut candidate = staged_path.clone();
    for suffix in 2.. {
        let target = LibraryPath::from_stored(candidate.clone()).resolve(&state.staging);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
        match rename_no_replace(&part, &target) {
            Ok(()) => break,
            // Never replaces a file: a name taken meanwhile gets a suffix.
            Err(error) if error.kind() == ErrorKind::AlreadyExists && suffix < 100 => {
                candidate = layout::with_suffix(&staged_path, suffix);
            }
            Err(error) => {
                return Err(ApiError::Internal(format!(
                    "cannot finish the upload: {error}"
                )));
            }
        }
    }
    let staged_path = candidate;
    sqlx::query(
        "UPDATE staged_files SET status = 'identifying', staged_path = ?
         WHERE id = ? AND status = 'receiving'",
    )
    .bind(&staged_path)
    .bind(id)
    .execute(&state.pool)
    .await?;
    metrics::counter!("heron_api_uploads_total").increment(1);
    tracing::info!(id, path = staged_path, "upload complete");
    Ok(())
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChoiceRequest {
    /// One of the file's candidate releases.
    Release {
        release_id: String,
        recording_id: String,
    },
    /// Keep the file's own tags.
    Own,
}

/// Imports a file as one of its candidate releases, or with its own tags:
/// heron-librarian works out its tags and path again. Operators and admins.
#[utoipa::path(post, path = "/staging/files/{id}/choice", tag = "buffer",
    params(("id" = u64, Path)), request_body = ChoiceRequest,
    responses((status = 200, body = StagedFile), (status = "4XX", body = ErrorBody)))]
async fn choose(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(request): Json<ChoiceRequest>,
) -> ApiResult<Json<StagedFile>> {
    user.require(Role::Operator)?;
    let file = find_file(&state, id).await?;
    if file.status != StagedStatus::Ready {
        return Err(ApiError::Conflict(
            "only files ready to import can be changed".to_owned(),
        ));
    }
    let choice = match request {
        ChoiceRequest::Own => StagedChoice::Own,
        ChoiceRequest::Release {
            release_id,
            recording_id,
        } => {
            let Some(candidate) = file.candidates.iter().find(|candidate| {
                candidate.release_id == release_id && candidate.recording_id == recording_id
            }) else {
                return Err(ApiError::Invalid(
                    "pick one of the file's candidate releases".to_owned(),
                ));
            };
            StagedChoice::Release {
                release_id,
                recording_id,
                disc: candidate.disc,
                track: candidate.track,
            }
        }
    };
    let json =
        serde_json::to_string(&choice).map_err(|error| ApiError::Internal(error.to_string()))?;
    let updated = sqlx::query(
        "UPDATE staged_files SET choice = ?, status = 'identifying', error = NULL
         WHERE id = ? AND status = 'ready'",
    )
    .bind(json)
    .bind(id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(ApiError::Conflict(
            "only files ready to import can be changed".to_owned(),
        ));
    }
    Ok(Json(find_file(&state, id).await?))
}

/// A file of the buffer zone, to listen to it. Supports range requests.
#[utoipa::path(get, path = "/staging/files/{id}/audio", tag = "buffer",
    params(("id" = u64, Path)),
    responses(
        (status = 200, description = "The file", content_type = "application/octet-stream"),
        (status = 206, description = "Part of the file"),
        (status = "4XX", body = ErrorBody)))]
async fn get_staged_audio(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<u64>,
    request: Request,
) -> ApiResult<Response> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT staged_path, CAST(status AS CHAR) FROM staged_files WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let (path, status) = row.ok_or_else(|| ApiError::not_found("file"))?;
    if !matches!(
        status.as_str(),
        "identifying" | "ready" | "failed" | "importing"
    ) {
        return Err(ApiError::not_found("file"));
    }
    let file: PathBuf = LibraryPath::from_stored(path).resolve(&state.staging);
    let service = tower_http::services::ServeFile::new(file);
    let (parts, _) = request.into_parts();
    let request = Request::from_parts(parts, Body::empty());
    let response = tower::ServiceExt::oneshot(service, request)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(ApiError::not_found("file"));
    }
    Ok(response.map(Body::new))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FileSelection {
    pub file_ids: Vec<u64>,
}

fn selection(selection: &FileSelection) -> ApiResult<Vec<u64>> {
    let mut ids = selection.file_ids.clone();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() || ids.len() > MAX_FILES {
        return Err(ApiError::Invalid(format!("select 1 to {MAX_FILES} files")));
    }
    Ok(ids)
}

/// Imports files ready in the buffer zone: heron-librarian tags them and
/// files them in the library as one batch, which can be undone. Files that
/// are not ready are skipped (see the task's report). Operators and admins.
#[utoipa::path(post, path = "/staging/import", tag = "buffer",
    request_body = FileSelection,
    responses((status = 202, body = LibraryTask), (status = "4XX", body = ErrorBody)))]
async fn import_files(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(request): Json<FileSelection>,
) -> ApiResult<(StatusCode, Json<LibraryTask>)> {
    user.require(Role::Operator)?;
    let file_ids = selection(&request)?;
    let task = ask(&state, &user, TaskRequest::StagingImport { file_ids }).await?;
    Ok((StatusCode::ACCEPTED, Json(task)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Discarded {
    /// Files removed from the buffer zone.
    pub discarded: u64,
}

/// Removes files from the buffer zone (uploads in progress too). They never
/// reached the library. Files being imported are left alone. Operators and
/// admins.
#[utoipa::path(post, path = "/staging/discard", tag = "buffer",
    request_body = FileSelection,
    responses((status = 200, body = Discarded), (status = "4XX", body = ErrorBody)))]
async fn discard_files(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(request): Json<FileSelection>,
) -> ApiResult<Json<Discarded>> {
    user.require(Role::Operator)?;
    let mut discarded = 0;
    for id in selection(&request)? {
        let row: Option<(u64, String)> =
            sqlx::query_as("SELECT group_id, staged_path FROM staged_files WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?;
        let Some((group_id, staged_path)) = row else {
            continue;
        };
        // The librarian claims files it imports the same way, so a file is
        // either discarded or imported.
        let updated = sqlx::query(
            "UPDATE staged_files SET status = 'discarded'
             WHERE id = ? AND status IN ('receiving', 'identifying', 'ready', 'failed')",
        )
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();
        if updated == 0 {
            continue;
        }
        discarded += 1;
        for path in [
            LibraryPath::from_stored(staged_path).resolve(&state.staging),
            state
                .staging
                .join(group_id.to_string())
                .join(upload_part_name(id)),
        ] {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    if let Some(parent) = path.parent() {
                        remove_empty_dirs(parent, &state.staging);
                    }
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => tracing::warn!(path = %path.display(), %error, "cannot discard"),
            }
        }
    }
    tracing::info!(user = %user.username, discarded, "files discarded from the buffer zone");
    Ok(Json(Discarded { discarded }))
}

/// Renames without ever replacing a file (a hard link and unlink where
/// the file system cannot).
fn rename_no_replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use rustix::fs::{CWD, RenameFlags};
    use rustix::io::Errno;
    match rustix::fs::renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
        Ok(()) => Ok(()),
        Err(errno)
            if errno == Errno::INVAL || errno == Errno::NOSYS || errno == Errno::OPNOTSUPP =>
        {
            std::fs::hard_link(from, to)?;
            std::fs::remove_file(from)
        }
        Err(errno) => Err(errno.into()),
    }
}

/// Removes `dir` and its empty parents, up to (not including) `root`.
fn remove_empty_dirs(dir: &std::path::Path, root: &std::path::Path) {
    let mut current = dir;
    while current != root && current.starts_with(root) {
        if std::fs::remove_dir(current).is_err() {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploaded_names_are_made_safe() {
        assert_eq!(
            safe_name("Album/01 Song.flac").as_deref(),
            Some("Album/01 Song.flac")
        );
        assert_eq!(
            safe_name("../../etc/passwd.mp3").as_deref(),
            Some("etc/passwd.mp3")
        );
        assert_eq!(safe_name("a\\b:c.mp3").as_deref(), Some("a/b_c.mp3"));
        assert_eq!(safe_name(".hidden.mp3").as_deref(), Some("_hidden.mp3"));
        assert_eq!(safe_name("//"), None);
    }
}
