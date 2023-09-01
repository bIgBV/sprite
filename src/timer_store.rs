use std::{env, time::SystemTime};

use anyhow::Result;
use sqlx::SqlitePool;
use tracing::{debug, instrument};

#[derive(Debug, Clone)]
pub struct TimerStore {
    pool: SqlitePool,
}

/// A Timer object
#[derive(Debug, sqlx::FromRow)]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Timer {
    /// The ID of this timer
    id: i64,

    /// The UID this timer is associated with
    unique_id: i64,

    /// When the timer was started
    start_time: u64,

    /// If this is the current timer associated with the [Timer::unique_id]
    is_current: bool,

    /// The duration for which this timer lasted.
    #[sqlx(default)]
    duration: Option<u64>,
}

impl TimerStore {
    pub async fn new() -> Result<Self> {
        let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;
        Ok(TimerStore { pool })
    }

    #[instrument(skip(self))]
    pub async fn create_timer(&self, uid: i64) -> Result<i64> {
        debug!(uid, message = "Creating timer");

        // The start_time field has defaults to the current unix epoch
        let id = sqlx::query!(
            r#"
INSERT INTO TIMERS (UNIQUE_ID, IS_CURRENT)
VALUES (?1, true)"#,
            uid
        )
        .execute(&self.pool)
        .await?
        .last_insert_rowid();

        Ok(id)
    }
}
