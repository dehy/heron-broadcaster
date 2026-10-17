// SPDX-License-Identifier: AGPL-3.0-or-later

//! The library lock, held by the scanner during a scan and by the librarian
//! while it moves a file or writes tags and records it in the database.
//!
//! Without it, a scan running during a move could see the file at neither
//! path (and mark the track missing), or at the old path after the
//! database already points at the new one (and "move" it back).
//!
//! It is a MariaDB named lock (`GET_LOCK`), held by one connection: if the
//! process holding it dies, the server drops the connection and releases
//! the lock, so a crash never leaves the library locked.

use std::time::Duration;

use sqlx::MySqlPool;
use sqlx::pool::PoolConnection;

/// How long one `GET_LOCK` call waits before logging and trying again.
const WAIT_STEP: Duration = Duration::from_secs(30);

/// A held library lock. Release it with [`LibraryLock::release`]; dropping
/// it closes its connection, which releases it too.
pub struct LibraryLock {
    connection: PoolConnection<sqlx::MySql>,
}

impl LibraryLock {
    /// Waits for the lock. `holder` names the caller in logs.
    pub async fn acquire(pool: &MySqlPool, holder: &str) -> Result<Self, sqlx::Error> {
        let mut connection = pool.acquire().await?;
        // Never returned to the pool: a connection still holding the lock
        // (after a panic or a cancelled future) must not be reused.
        connection.close_on_drop();
        loop {
            // Named locks are server-wide: scope the name to the database.
            let acquired: Option<i32> =
                sqlx::query_scalar("SELECT GET_LOCK(CONCAT(DATABASE(), '.heron.library'), ?)")
                    .bind(WAIT_STEP.as_secs())
                    .fetch_one(&mut *connection)
                    .await?;
            match acquired {
                Some(1) => return Ok(Self { connection }),
                Some(_) => tracing::info!(holder, "still waiting for the library lock"),
                None => {
                    return Err(sqlx::Error::Protocol(
                        "GET_LOCK failed on the library lock".into(),
                    ));
                }
            }
        }
    }

    pub async fn release(mut self) -> Result<(), sqlx::Error> {
        sqlx::query("DO RELEASE_LOCK(CONCAT(DATABASE(), '.heron.library'))")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use heron_service::db::ScratchDatabase;

    #[tokio::test]
    async fn the_lock_is_exclusive_and_freed_on_drop() {
        let Some(db) = ScratchDatabase::create().await else {
            return;
        };
        let first = LibraryLock::acquire(&db.pool, "first").await.unwrap();
        let second = tokio::time::timeout(
            Duration::from_millis(300),
            LibraryLock::acquire(&db.pool, "second"),
        )
        .await;
        assert!(second.is_err(), "the lock is held");

        // A holder that disappears without releasing frees the lock.
        drop(first);
        let second = tokio::time::timeout(
            Duration::from_secs(10),
            LibraryLock::acquire(&db.pool, "second"),
        )
        .await
        .expect("freed when the holder's connection closed")
        .unwrap();
        second.release().await.unwrap();
        LibraryLock::acquire(&db.pool, "third")
            .await
            .unwrap()
            .release()
            .await
            .unwrap();
        db.drop().await;
    }
}
