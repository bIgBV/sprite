use std::{
    alloc::System,
    env,
    time::{Duration, SystemTime},
};

use anyhow::Result;
use sqlx::SqlitePool;
use tracing::{debug, error, instrument};

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
    start_time: i64,

    /// If this is the current timer associated with the [Timer::unique_id]
    is_current: bool,

    /// The duration for which this timer lasted.
    #[sqlx(default)]
    duration: Option<i64>,
}

impl TimerStore {
    pub async fn new() -> Result<Self> {
        let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;
        Ok(TimerStore { pool })
    }

    async fn new_test(pool: SqlitePool) -> Result<Self> {
        Ok(TimerStore { pool })
    }

    /// Toggles the current timer for the given UID
    #[instrument(skip(self))]
    pub async fn toggle_current(&self, uid: i64) -> Result<i64> {
        if let Some(mut timer) = self.current_timer(uid).await {
            // We already have an existing timer
            let timer_id = timer.id;
            debug!(?timer, "Ending current timer");
            let timer_duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?
                - Duration::from_secs(timer.start_time.try_into()?);
            timer.duration = Some(timer_duration.as_secs().try_into()?);

            if !self.update_timer(timer).await? {
                error!(?timer_id, "Error updating timer");
                return Err(anyhow::anyhow!("Unable to update timer"));
            }

            Ok(timer_id)
        } else {
            // The start_time field has defaults to the current unix epoch
            self.create_timer(uid).await
        }
    }

    async fn create_timer(&self, uid: i64) -> Result<i64> {
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

    async fn update_timer(&self, timer: Timer) -> Result<bool> {
        let rows = sqlx::query!(
            r#"
UPDATE TIMERS
set is_current = false, duration = ?1
where id = ?2
            "#,
            timer.duration,
            timer.id
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        Ok(rows == 1)
    }

    async fn current_timer(&self, uid: i64) -> Option<Timer> {
        sqlx::query_as::<sqlx::sqlite::Sqlite, Timer>(
            r#"
SELECT * FROM TIMERS
WHERE unique_id = ?1 AND is_current"#,
        )
        .bind(uid)
        .fetch_one(&self.pool)
        .await
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use pretty_assertions::assert_eq;
    use sqlx::sqlite::SqliteConnectOptions;
    use tracing_test::traced_test;

    async fn setup() -> Result<TimerStore> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?;
        let pool = SqlitePool::connect_with(options).await?;

        sqlx::migrate!().run(&pool).await?;
        let store = TimerStore::new_test(pool).await?;
        Ok(store)
    }

    #[traced_test]
    #[tokio::test]
    async fn toggle_create_when_not_exist() {
        let store = setup().await.unwrap();
        let result = store.toggle_current(1).await.unwrap();

        assert_eq!(result, 1);
    }
}
