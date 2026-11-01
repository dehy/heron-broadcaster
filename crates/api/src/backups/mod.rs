// SPDX-License-Identifier: AGPL-3.0-or-later

//! Backups (docs/backups.md): every day, and on request, the configuration
//! file (everything set by hand) and a dump of the database, in the backup
//! folder; restoring a configuration file.

mod dump;
pub mod file;

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::app::AppState;
use crate::auth::{CurrentUser, Role};
use crate::error::{ApiError, ApiResult, ErrorBody};
use crate::settings::StationSettings;
use crate::stations::Stations;
pub use file::{ConfigFile, Kind, RestoreReport};

/// How many backups taken before a restore are kept.
const KEEP_BEFORE_RESTORE: usize = 7;
/// The largest configuration file uploaded.
const MAX_UPLOAD: usize = 512 * 1024 * 1024;
/// Locked by a backup or restore, in the backup folder: one at a time
/// across processes, the API and `heron-api backup`.
const LOCK_FILE: &str = ".heron-backup.lock";

/// The `backups` settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(default, deny_unknown_fields)]
pub struct BackupSettings {
    /// The daily backup.
    pub enabled: bool,
    /// `HH:MM`, in the first station's time zone.
    pub time: String,
    /// Daily backups kept: 1 to 90.
    pub keep_days: u32,
    /// A database dump with each backup.
    pub database: bool,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            time: "03:30".to_owned(),
            keep_days: 7,
            database: true,
        }
    }
}

impl BackupSettings {
    pub const SETTINGS_NAME: &'static str = "backups";

    fn at(&self) -> Option<(i8, i8)> {
        let (hour, minute) = self.time.split_once(':')?;
        let hour: i8 = hour.parse().ok()?;
        let minute: i8 = minute.parse().ok()?;
        ((0..24).contains(&hour) && (0..60).contains(&minute) && self.time.len() == 5)
            .then_some((hour, minute))
    }

    pub fn check(&self) -> ApiResult<()> {
        if self.at().is_none() {
            return Err(ApiError::Invalid(
                "time is HH:MM, 00:00 to 23:59".to_owned(),
            ));
        }
        if !(1..=90).contains(&self.keep_days) {
            return Err(ApiError::Invalid("keep_days is 1 to 90".to_owned()));
        }
        Ok(())
    }
}

/// A backup in the folder.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct BackupView {
    /// Its time and kind: `2026-11-03T03-30-00Z-daily`.
    pub name: String,
    pub kind: Kind,
    pub created_at_unix_ms: i64,
    /// Bytes.
    pub config_size: u64,
    /// Bytes; missing without a dump.
    pub database_size: Option<u64>,
    /// Why the dump failed.
    pub database_error: Option<String>,
}

/// What keeps backups from being written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct FolderProblem {
    /// The system's error (`Permission denied`, `No space left on device`…).
    pub error: String,
    /// The folder is not the API's user's: `chown` fixes it.
    pub permission_denied: bool,
    /// The user and group the API runs as: the backup folder must be
    /// theirs.
    pub uid: u32,
    pub gid: u32,
}

/// The backup folder, and what backs up the database.
pub struct Backups {
    dir: PathBuf,
    database_url: String,
    /// One backup or restore at a time in this process; `LOCK_FILE` for
    /// the others.
    lock: tokio::sync::Mutex<()>,
    /// Whether the folder could be written, when last checked.
    problem: std::sync::Mutex<Option<FolderProblem>>,
}

/// An error writing to the backup folder, saying how to fix it.
fn folder_error(dir: &Path, error: &std::io::Error) -> ApiError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        let (uid, gid) = ids();
        ApiError::Invalid(format!(
            "cannot write in the backup folder ({}): it is not the API's user's ({uid}:{gid}). \
             On the host, give it to that user: chown {uid}:{gid} on HERON_BACKUP_FOLDER \
             (./data/backups by default)",
            dir.display()
        ))
    } else {
        ApiError::Invalid(format!(
            "cannot write in the backup folder ({}): {error}",
            dir.display()
        ))
    }
}

fn ids() -> (u32, u32) {
    (
        rustix::process::getuid().as_raw(),
        rustix::process::getgid().as_raw(),
    )
}

fn config_file(name: &str) -> String {
    format!("heron-config-{name}.json")
}

fn database_file(name: &str) -> String {
    format!("heron-database-{name}.sql.gz")
}

fn failed_file(name: &str) -> String {
    format!("heron-database-{name}.failed")
}

/// Whether `name` is a backup's name (`<time>-<kind>`): nothing else is
/// read or deleted.
fn parse_name(name: &str) -> Option<(Timestamp, Kind)> {
    // 2026-11-03T03-30-00Z
    let (time, kind) = (name.get(..20)?, name.get(21..)?);
    if name.as_bytes().get(20) != Some(&b'-') {
        return None;
    }
    let kind = Kind::parse(kind)?;
    let time = jiff::fmt::strtime::parse("%Y-%m-%dT%H-%M-%SZ", time)
        .ok()?
        .to_datetime()
        .ok()?
        .to_zoned(jiff::tz::TimeZone::UTC)
        .ok()?
        .timestamp();
    Some((time, kind))
}

/// Writes `bytes` to `path` whole, readable by this user only.
fn write_atomically(
    path: &Path,
    write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("backup");
    let temporary = path.with_file_name(format!(".{name}.tmp"));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        write(&mut file)?;
        file.flush()?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        if let Some(dir) = path.parent() {
            std::fs::File::open(dir)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

impl Backups {
    pub fn new(dir: PathBuf, database_url: String) -> Arc<Self> {
        Arc::new(Self {
            dir,
            database_url,
            lock: tokio::sync::Mutex::new(()),
            problem: std::sync::Mutex::new(None),
        })
    }

    /// What kept the folder from being written when last checked.
    pub fn problem(&self) -> Option<FolderProblem> {
        self.problem
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Writes and removes a file in the folder.
    fn probe(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        // A file of its own: another process checking at the same time
        // does not remove it.
        tempfile::Builder::new()
            .prefix(".heron-write-check-")
            .tempfile_in(&self.dir)?
            .close()
    }

    /// Whether the folder can be written, remembered for `/status`.
    pub fn check_folder(&self) -> std::io::Result<()> {
        let result = self.probe();
        let problem = result.as_ref().err().map(|error| {
            let (uid, gid) = ids();
            FolderProblem {
                error: error.to_string(),
                permission_denied: error.kind() == std::io::ErrorKind::PermissionDenied,
                uid,
                gid,
            }
        });
        match &problem {
            Some(problem) if problem.permission_denied => tracing::error!(
                dir = %self.dir.display(),
                error = %problem.error,
                "the backup folder cannot be written: give it to {}:{} on the host",
                problem.uid,
                problem.gid
            ),
            Some(problem) => tracing::error!(
                dir = %self.dir.display(),
                error = %problem.error,
                "the backup folder cannot be written"
            ),
            None => {}
        }
        let mut current = self
            .problem
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.is_some() && problem.is_none() {
            tracing::info!(dir = %self.dir.display(), "the backup folder can be written again");
        }
        current.clone_from(&problem);
        result
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The backups, newest first.
    pub fn list(&self) -> ApiResult<Vec<BackupView>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(folder_error(&self.dir, &error)),
        };
        let mut backups = Vec::new();
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name
                .to_str()
                .and_then(|name| name.strip_prefix("heron-config-"))
                .and_then(|name| name.strip_suffix(".json"))
            else {
                continue;
            };
            let Some((time, kind)) = parse_name(name) else {
                continue;
            };
            let size = |file: String| std::fs::metadata(self.dir.join(file)).ok().map(|m| m.len());
            backups.push(BackupView {
                name: name.to_owned(),
                kind,
                created_at_unix_ms: time.as_millisecond(),
                config_size: size(config_file(name)).unwrap_or(0),
                database_size: size(database_file(name)),
                database_error: std::fs::read_to_string(self.dir.join(failed_file(name)))
                    .ok()
                    .map(|error| error.trim().to_owned()),
            });
        }
        backups.sort_by(|a, b| b.name.cmp(&a.name));
        Ok(backups)
    }

    fn find(&self, name: &str) -> ApiResult<BackupView> {
        self.list()?
            .into_iter()
            .find(|backup| backup.name == name)
            .ok_or_else(|| ApiError::not_found(format!("backup {name}")))
    }

    /// Takes a backup: the configuration file, then the database dump.
    /// Waits for the backup or restore under way, here or in another
    /// process; the next one waits until both are dropped.
    async fn exclusive(
        &self,
    ) -> ApiResult<(tokio::sync::MutexGuard<'_, ()>, Option<std::fs::File>)> {
        let guard = self.lock.lock().await;
        self.check_folder()
            .map_err(|error| folder_error(&self.dir, &error))?;
        let path = self.dir.join(LOCK_FILE);
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .await
            .map_err(|error| folder_error(&self.dir, &error))?
            .into_std()
            .await;
        // Tried again and again rather than waited for in a thread: a task
        // stopped while it waits (the API shutting down) stops waiting.
        loop {
            match file.try_lock() {
                Ok(()) => return Ok((guard, Some(file))),
                Err(std::fs::TryLockError::WouldBlock) => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                // A share without locks: the API still takes one at a time.
                Err(std::fs::TryLockError::Error(error)) => {
                    tracing::warn!(%error, "cannot lock {}: backups taken on the command line may run at the same time", path.display());
                    return Ok((guard, None));
                }
            }
        }
    }

    pub async fn create(&self, pool: &MySqlPool, kind: Kind) -> ApiResult<BackupView> {
        let _exclusive = self.exclusive().await?;
        self.create_locked(pool, kind, None).await
    }

    async fn create_locked(
        &self,
        pool: &MySqlPool,
        kind: Kind,
        upload: Option<&[u8]>,
    ) -> ApiResult<BackupView> {
        self.check_folder()
            .map_err(|error| folder_error(&self.dir, &error))?;
        let mut now = Timestamp::now();
        // Each backup has its own second, so that names sort by time.
        let taken: Vec<String> = self.list()?.into_iter().map(|backup| backup.name).collect();
        let name = loop {
            let time = now.strftime("%Y-%m-%dT%H-%M-%SZ").to_string();
            if !taken.iter().any(|name| name.starts_with(&time)) {
                break format!("{time}-{}", kind.as_str());
            }
            now += Duration::from_secs(1);
        };
        let bytes = match upload {
            Some(bytes) => bytes.to_vec(),
            None => {
                let file = file::export(pool, kind, now.strftime("%Y-%m-%dT%H:%M:%SZ").to_string())
                    .await?;
                serde_json::to_vec_pretty(&file)
                    .map_err(|error| ApiError::Internal(error.to_string()))?
            }
        };
        let path = self.dir.join(config_file(&name));
        tokio::task::spawn_blocking(move || write_atomically(&path, |file| file.write_all(&bytes)))
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .map_err(|error| folder_error(&self.dir, &error))?;
        let settings: BackupSettings =
            crate::settings::load(pool, BackupSettings::SETTINGS_NAME).await?;
        if settings.database && kind != Kind::Uploaded {
            let target = self.dir.join(database_file(&name));
            match dump::dump(&self.database_url, &target).await {
                Ok(()) => {}
                Err(error) => {
                    tracing::error!(backup = %name, %error, "the database dump failed");
                    let failed = self.dir.join(failed_file(&name));
                    let _ = write_atomically(&failed, |file| file.write_all(error.as_bytes()));
                }
            }
        }
        tracing::info!(backup = %name, "backed up");
        self.find(&name)
    }

    /// Deletes a backup's files.
    pub fn delete(&self, name: &str) -> ApiResult<()> {
        self.find(name)?;
        for file in [config_file(name), database_file(name), failed_file(name)] {
            match std::fs::remove_file(self.dir.join(&file)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(ApiError::Internal(format!("cannot delete {file}: {error}")));
                }
            }
        }
        Ok(())
    }

    /// Deletes the daily backups beyond `keep_days`, and the backups taken
    /// before restores beyond the last few.
    fn prune(&self, keep_days: u32) -> ApiResult<()> {
        let backups = self.list()?;
        let old = |kind: Kind, keep: usize| {
            backups
                .iter()
                .filter(move |backup| backup.kind == kind)
                .skip(keep)
        };
        for backup in old(Kind::Daily, keep_days as usize)
            .chain(old(Kind::BeforeRestore, KEEP_BEFORE_RESTORE))
        {
            self.delete(&backup.name)?;
            tracing::info!(backup = %backup.name, "old backup deleted");
        }
        Ok(())
    }

    fn read(&self, name: &str) -> ApiResult<Vec<u8>> {
        self.find(name)?;
        std::fs::read(self.dir.join(config_file(name)))
            .map_err(|error| ApiError::Internal(format!("cannot read the backup: {error}")))
    }

    /// Restores a configuration file, after backing up the configuration.
    pub async fn restore(
        &self,
        pool: &MySqlPool,
        stations: Option<&Stations>,
        bytes: &[u8],
        upload: bool,
    ) -> ApiResult<RestoreReport> {
        let _exclusive = self.exclusive().await?;
        let file: ConfigFile = serde_json::from_slice(bytes).map_err(|error| {
            ApiError::Invalid(format!("this is not a Heron configuration file: {error}"))
        })?;
        let schema: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success",
        )
        .fetch_one(pool)
        .await?;
        file::check(&file, schema)?;
        if upload {
            self.create_locked(pool, Kind::Uploaded, Some(bytes))
                .await?;
        }
        let before = self.create_locked(pool, Kind::BeforeRestore, None).await?;
        let mut report = RestoreReport {
            before_restore: before.name,
            ..RestoreReport::default()
        };
        // Read committed: what other tasks change meanwhile (a webhook's
        // deliveries, retried in the background) is read as it is now. In
        // repeatable read, deleting a row changed since the transaction's
        // snapshot fails ("Record has changed since last read").
        let mut connection = pool.acquire().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *connection)
            .await?;
        let mut tx = sqlx::Connection::begin(&mut *connection).await?;
        file::restore(&mut tx, &file, &mut report).await?;
        // Everyone signs in again, with the restored users.
        sqlx::query("DELETE FROM sessions")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        tracing::info!(before = %report.before_restore, "configuration restored");
        if let Some(stations) = stations {
            let ids: Vec<u64> = sqlx::query_scalar("SELECT id FROM stations ORDER BY id")
                .fetch_all(pool)
                .await?;
            for id in ids {
                if let Err(error) = stations.reload(id).await {
                    tracing::error!(station = id, %error, "cannot restart the station after the restore");
                }
            }
            for station in stations.all() {
                station.sync.changed();
            }
        }
        let settings: BackupSettings =
            crate::settings::load(pool, BackupSettings::SETTINGS_NAME).await?;
        self.prune(settings.keep_days)?;
        Ok(report)
    }

    /// Takes the daily backup at its time, and catches up a day missed.
    pub async fn run_daily(self: Arc<Self>, pool: MySqlPool, stations: Arc<Stations>) {
        loop {
            if let Err(error) = self.daily(&pool, &stations).await {
                tracing::error!(%error, "the daily backup failed");
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    async fn daily(&self, pool: &MySqlPool, stations: &Stations) -> ApiResult<()> {
        // Checked every minute: the banner goes once the folder is fixed.
        if self.check_folder().is_err() {
            return Ok(());
        }
        let settings: BackupSettings =
            crate::settings::load(pool, BackupSettings::SETTINGS_NAME).await?;
        let Some((hour, minute)) = settings.at().filter(|_| settings.enabled) else {
            return Ok(());
        };
        let zone = crate::settings::load_for::<StationSettings>(
            pool,
            stations.first().id(),
            StationSettings::SETTINGS_NAME,
        )
        .await?
        .time_zone();
        let due = last_due(&Timestamp::now().to_zoned(zone), hour, minute)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        let last = self
            .list()?
            .into_iter()
            .find(|backup| backup.kind == Kind::Daily)
            .map(|backup| backup.created_at_unix_ms);
        if last.is_some_and(|last| last >= due.timestamp().as_millisecond()) {
            return Ok(());
        }
        self.create(pool, Kind::Daily).await?;
        self.prune(settings.keep_days)
    }
}

/// When the daily backup was last due: today at its time, or yesterday
/// before it. A daily backup older than that is missing.
fn last_due(now: &jiff::Zoned, hour: i8, minute: i8) -> Result<jiff::Zoned, jiff::Error> {
    let today = now
        .with()
        .hour(hour)
        .minute(minute)
        .second(0)
        .subsec_nanosecond(0)
        .build()?;
    if today <= *now {
        Ok(today)
    } else {
        today.yesterday()
    }
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_backups, create_backup))
        .routes(routes!(delete_backup))
        .routes(routes!(download_config))
        .routes(routes!(download_database))
        .routes(routes!(restore_backup))
        .routes(routes!(upload_and_restore))
        .routes(routes!(get_backup_settings, put_backup_settings))
        // A configuration file with every track's settings is large.
        .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD))
}

/// The backups, newest first. Admins only.
#[utoipa::path(get, path = "/backups", tag = "backups",
    responses((status = 200, body = Vec<BackupView>), (status = "4XX", body = ErrorBody)))]
async fn list_backups(
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<BackupView>>> {
    user.require(Role::Admin)?;
    state.backups.list().map(Json)
}

/// Backs up now: the configuration file and the database dump. Admins only.
#[utoipa::path(post, path = "/backups", tag = "backups",
    responses((status = 201, body = BackupView), (status = "4XX", body = ErrorBody)))]
async fn create_backup(
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<(StatusCode, Json<BackupView>)> {
    user.require(Role::Admin)?;
    let backup = state.backups.create(&state.pool, Kind::Manual).await?;
    tracing::info!(user = %user.username, backup = %backup.name, "backed up on request");
    Ok((StatusCode::CREATED, Json(backup)))
}

/// Deletes a backup's files. Admins only.
#[utoipa::path(delete, path = "/backups/{name}", tag = "backups", params(("name" = String, Path)),
    responses((status = 204), (status = "4XX", body = ErrorBody)))]
async fn delete_backup(
    user: CurrentUser,
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> ApiResult<StatusCode> {
    user.require(Role::Admin)?;
    state.backups.delete(&name)?;
    tracing::info!(user = %user.username, backup = %name, "backup deleted");
    Ok(StatusCode::NO_CONTENT)
}

fn download(path: PathBuf, file_name: String, content_type: &'static str) -> ApiResult<Response> {
    let bytes = std::fs::read(&path)
        .map_err(|error| ApiError::Internal(format!("cannot read {file_name}: {error}")))?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// Downloads a backup's configuration file. Admins only: it holds secrets.
#[utoipa::path(get, path = "/backups/{name}/config", tag = "backups", params(("name" = String, Path)),
    responses((status = 200, content_type = "application/json"), (status = "4XX", body = ErrorBody)))]
async fn download_config(
    user: CurrentUser,
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> ApiResult<Response> {
    user.require(Role::Admin)?;
    state.backups.find(&name)?;
    let file = config_file(&name);
    download(state.backups.dir().join(&file), file, "application/json")
}

/// Downloads a backup's database dump. Admins only: it holds secrets.
#[utoipa::path(get, path = "/backups/{name}/database", tag = "backups", params(("name" = String, Path)),
    responses((status = 200, content_type = "application/gzip"), (status = "4XX", body = ErrorBody)))]
async fn download_database(
    user: CurrentUser,
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> ApiResult<Response> {
    user.require(Role::Admin)?;
    let backup = state.backups.find(&name)?;
    if backup.database_size.is_none() {
        return Err(ApiError::not_found(format!("the database dump of {name}")));
    }
    let file = database_file(&name);
    download(state.backups.dir().join(&file), file, "application/gzip")
}

/// Restores a backup's configuration file, after backing up the current
/// configuration. Everyone signs in again. Admins only.
#[utoipa::path(post, path = "/backups/{name}/restore", tag = "backups", params(("name" = String, Path)),
    responses((status = 200, body = RestoreReport), (status = "4XX", body = ErrorBody)))]
async fn restore_backup(
    user: CurrentUser,
    State(state): State<AppState>,
    UrlPath(name): UrlPath<String>,
) -> ApiResult<Json<RestoreReport>> {
    user.require(Role::Admin)?;
    let bytes = state.backups.read(&name)?;
    let report = state
        .backups
        .restore(&state.pool, Some(&state.stations), &bytes, false)
        .await?;
    tracing::info!(user = %user.username, backup = %name, "backup restored");
    Ok(Json(report))
}

/// Restores an uploaded configuration file (the request's body), kept as an
/// `uploaded` backup. Admins only.
#[utoipa::path(post, path = "/backups/restore", tag = "backups",
    request_body(content = String, content_type = "application/json", description = "A configuration file"),
    responses((status = 200, body = RestoreReport), (status = "4XX", body = ErrorBody)))]
async fn upload_and_restore(
    user: CurrentUser,
    State(state): State<AppState>,
    body: Bytes,
) -> ApiResult<Json<RestoreReport>> {
    user.require(Role::Admin)?;
    let report = state
        .backups
        .restore(&state.pool, Some(&state.stations), &body, true)
        .await?;
    tracing::info!(user = %user.username, "uploaded backup restored");
    Ok(Json(report))
}

#[utoipa::path(get, path = "/settings/backups", tag = "settings",
    responses((status = 200, body = BackupSettings), (status = "4XX", body = ErrorBody)))]
async fn get_backup_settings(
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Json<BackupSettings>> {
    user.require(Role::Admin)?;
    crate::settings::load(&state.pool, BackupSettings::SETTINGS_NAME)
        .await
        .map(Json)
}

#[utoipa::path(put, path = "/settings/backups", tag = "settings", request_body = BackupSettings,
    responses((status = 200, body = BackupSettings), (status = "4XX", body = ErrorBody)))]
async fn put_backup_settings(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(settings): Json<BackupSettings>,
) -> ApiResult<Json<BackupSettings>> {
    user.require(Role::Admin)?;
    settings.check()?;
    crate::settings::save(&state.pool, BackupSettings::SETTINGS_NAME, &settings).await?;
    tracing::info!(user = %user.username, ?settings, "backup settings changed");
    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_carry_the_time_and_kind() {
        let (time, kind) = parse_name("2026-11-03T03-30-00Z-before_restore").unwrap();
        assert_eq!(kind, Kind::BeforeRestore);
        assert_eq!(time.to_string(), "2026-11-03T03:30:00Z");
        assert!(parse_name("2026-11-03T03-30-00Z-weekly").is_none());
        assert!(parse_name("../../etc/passwd").is_none());
        assert!(parse_name("2026-11-03T03-30-00Z").is_none());
    }

    #[test]
    fn the_daily_backup_is_due_at_its_time_or_caught_up() {
        let zone = jiff::tz::TimeZone::get("Europe/Paris").unwrap();
        let at = |time: &str| {
            jiff::civil::DateTime::strptime("%Y-%m-%d %H:%M", time)
                .unwrap()
                .to_zoned(zone.clone())
                .unwrap()
        };
        assert_eq!(
            last_due(&at("2026-11-03 04:00"), 3, 30).unwrap(),
            at("2026-11-03 03:30")
        );
        assert_eq!(
            last_due(&at("2026-11-03 03:30"), 3, 30).unwrap(),
            at("2026-11-03 03:30")
        );
        // Before its time: yesterday's, caught up if it was missed.
        assert_eq!(
            last_due(&at("2026-11-03 02:00"), 3, 30).unwrap(),
            at("2026-11-02 03:30")
        );
    }

    #[test]
    fn old_daily_and_before_restore_backups_are_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path().to_path_buf(), String::new());
        for day in 1..=9 {
            for kind in ["daily", "before_restore", "manual"] {
                let name = format!("2026-11-{day:02}T03-30-00Z-{kind}");
                std::fs::write(dir.path().join(config_file(&name)), "{}").unwrap();
                std::fs::write(dir.path().join(database_file(&name)), "").unwrap();
            }
        }
        backups.prune(7).unwrap();
        let left = backups.list().unwrap();
        let days = |kind: Kind| -> Vec<String> {
            left.iter()
                .filter(|backup| backup.kind == kind)
                .map(|backup| backup.name[8..10].to_owned())
                .collect()
        };
        assert_eq!(
            days(Kind::Daily),
            ["09", "08", "07", "06", "05", "04", "03"]
        );
        assert_eq!(days(Kind::BeforeRestore).len(), KEEP_BEFORE_RESTORE);
        assert_eq!(days(Kind::Manual).len(), 9);
        // A backup's dump goes with it.
        assert!(
            !dir.path()
                .join(database_file("2026-11-01T03-30-00Z-daily"))
                .exists()
        );
        assert!(
            dir.path()
                .join(database_file("2026-11-01T03-30-00Z-manual"))
                .exists()
        );
    }

    #[test]
    fn a_folder_that_cannot_be_written_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the folder should be: it cannot be created.
        let taken = dir.path().join("backups");
        std::fs::write(&taken, "").unwrap();
        let backups = Backups::new(taken.clone(), String::new());
        assert!(backups.check_folder().is_err());
        let problem = backups.problem().unwrap();
        assert_eq!((problem.uid, problem.gid), ids());
        assert!(!problem.permission_denied, "not a matter of owner");
        // Fixed: the problem goes at the next check.
        std::fs::remove_file(&taken).unwrap();
        assert!(backups.check_folder().is_ok());
        assert_eq!(backups.problem(), None);
        assert_eq!(
            std::fs::read_dir(&taken).unwrap().count(),
            0,
            "the probe is gone"
        );
    }

    #[test]
    fn permission_errors_say_how_to_fix_them() {
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let message = folder_error(Path::new("/var/lib/heron/backups"), &denied).to_string();
        let (uid, gid) = ids();
        assert!(message.contains(&format!("chown {uid}:{gid}")), "{message}");
        assert!(message.contains("HERON_BACKUP_FOLDER"), "{message}");
    }

    #[test]
    fn settings_are_checked() {
        assert!(BackupSettings::default().check().is_ok());
        for time in ["3:30", "24:00", "03:60", "03:30:00"] {
            let settings = BackupSettings {
                time: time.to_owned(),
                ..BackupSettings::default()
            };
            assert!(settings.check().is_err(), "{time}");
        }
        let settings = BackupSettings {
            keep_days: 0,
            ..BackupSettings::default()
        };
        assert!(settings.check().is_err());
    }

    #[test]
    fn files_are_written_whole_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("heron-config-x.json");
        write_atomically(&path, |file| file.write_all(b"{}")).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        let mode = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        // A failed write leaves nothing under the name, nor its temporary file.
        let failed = write_atomically(&dir.path().join("other.json"), |_| {
            Err(std::io::Error::other("disk full"))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn one_backup_at_a_time_across_processes() {
        let dir = tempfile::tempdir().unwrap();
        // Two processes: each its own `Backups`, the folder in common.
        let api = Backups::new(dir.path().to_owned(), String::new());
        let command_line = Backups::new(dir.path().to_owned(), String::new());
        let held = api.exclusive().await.unwrap();
        assert!(held.1.is_some(), "locked");
        let waiting = tokio::spawn(async move { command_line.exclusive().await.map(|_| ()) });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!waiting.is_finished(), "waits for the other");
        // Stopped while it waits (the API shutting down): it stops at once.
        let stopped = Backups::new(dir.path().to_owned(), String::new());
        let gone = tokio::time::timeout(Duration::from_millis(300), stopped.exclusive()).await;
        assert!(gone.is_err(), "still waiting when stopped");
        drop(held);
        tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("goes on once the other is done")
            .unwrap()
            .unwrap();
    }
}
