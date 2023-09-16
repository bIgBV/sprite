use std::{
    env,
    time::{Duration, SystemTime},
};

use anyhow::Result;

use serde::Serialize;
use sqlx::SqlitePool;
use tracing::{debug, error, instrument};

use crate::uid::TagId;

#[derive(Debug, Clone)]
pub struct TimerStore {
    pool: SqlitePool,
}

/// A Timer object
#[derive(Debug, sqlx::FromRow, Default, Serialize)]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Timer {
    /// The ID of this timer
    id: i64,

    /// The TagId this timer is associated with
    unique_id: String,

    /// When the timer was started
    pub start_time: i64,

    /// If this is the current timer associated with the [Timer::unique_id]
    pub is_current: bool,

    /// The duration for which this timer lasted.
    #[sqlx(default)]
    pub duration: Option<i64>,

    /// Timestamp of when the timer was stopped. Calculated as start_time + duration
    #[sqlx(skip)]
    pub end_time: Option<i64>,
}

impl Timer {
    pub fn update_end_time(mut self) -> Result<Self> {
        if let Some(duration) = self.duration {
            // Only want to set end_time for a timer which has already been stopped
            self.end_time = Some(self.start_time + duration);
        }

        Ok(self)
    }
}

impl TimerStore {
    pub async fn new() -> Result<Self> {
        let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(TimerStore { pool })
    }

    #[cfg(test)]
    async fn new_test(pool: SqlitePool) -> Result<Self> {
        Ok(TimerStore { pool })
    }

    /// Toggles the current timer for the given UID
    #[instrument(skip(self))]
    pub async fn toggle_current(&self, uid: &TagId) -> Result<i64> {
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
            debug!(tag_id = uid.as_ref(), "Creating new timer");
            // The start_time field has defaults to the current unix epoch
            self.create_timer(uid).await
        }
    }

    #[cfg(test)]
    async fn get_timer(&self, timer_id: i64) -> Result<Timer> {
        Ok(sqlx::query_as::<sqlx::sqlite::Sqlite, Timer>(
            r#"
SELECT * FROM TIMERS
WHERE id = ?1"#,
        )
        .bind(timer_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_timers_by_tag(&self, timer_tag: &TagId) -> Result<Vec<Timer>> {
        let result = sqlx::query_as::<sqlx::Sqlite, Timer>(
            r#"
SELECT * FROM TIMERS
WHERE unique_id = ?1
ORDER BY start_time DESC
            "#,
        )
        .bind(timer_tag.as_ref())
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_exportable_timers_by_tag(&self, timer_tag: &TagId) -> Result<Vec<Timer>> {
        let result = sqlx::query_as::<sqlx::Sqlite, Timer>(
            r#"
SELECT * FROM TIMERS
WHERE unique_id = ?1 AND IS_CURRENT = 0
ORDER BY start_time DESC
            "#,
        )
        .bind(timer_tag.as_ref())
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    /// Creates a new timer with the start time set to the unix epoch in UTC
    async fn create_timer(&self, uid: &TagId) -> Result<i64> {
        let tag_id = uid.as_ref();

        let start_epoch = chrono::Utc::now().timestamp();

        let id = sqlx::query!(
            r#"
INSERT INTO TIMERS (UNIQUE_ID, IS_CURRENT, START_TIME)
VALUES (?1, 1, ?2)"#,
            tag_id,
            start_epoch
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
set is_current = 0, duration = ?1
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

    async fn current_timer(&self, uid: &TagId) -> Option<Timer> {
        sqlx::query_as::<sqlx::sqlite::Sqlite, Timer>(
            r#"
SELECT * FROM TIMERS
WHERE unique_id = ?1 AND is_current = 1"#,
        )
        .bind(uid.as_ref())
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
    async fn toggle_create_when_timer_does_not_exist() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();
        let result = store.toggle_current(&uid).await.unwrap();

        assert_eq!(result, 1);
    }

    #[traced_test]
    #[tokio::test]
    async fn toggle_end_timer_when_current_already_exists() {
        let uid = TagId::new("test-tag").unwrap();
        let store = setup().await.unwrap();
        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        let timer = store.get_timer(result).await.unwrap();

        let Some(timer_duration) = timer.duration else {
            panic!("Timer hasn't been turned off");
        };

        assert!(timer_duration >= 2);
        assert!(!timer.is_current)
    }

    #[traced_test]
    #[tokio::test]
    async fn toggle_timer_creates_new_current_timer() {
        let uid = TagId::new("test-tag").unwrap();
        let store = setup().await.unwrap();
        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        let timer = store.get_timer(result).await.unwrap();

        let Some(timer_duration) = timer.duration else {
            panic!("Timer hasn't been turned off");
        };

        assert!(timer_duration >= 2);
        assert!(!timer.is_current);

        let timer_id = store.toggle_current(&uid).await.unwrap();
        assert_eq!(timer_id, 2);
    }

    #[traced_test]
    #[tokio::test]
    async fn get_tiemrs_returns_timers_by_tag_id() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();

        for _ in 0..20 {
            store.toggle_current(&uid).await.unwrap();
            store.toggle_current(&uid).await.unwrap();
        }

        let timers = store.get_timers_by_tag(&uid).await.unwrap();

        assert_eq!(timers.len(), 20);
    }

    #[traced_test]
    #[tokio::test]
    async fn get_exportable_timers_by_tag_returns_only_complete_timers() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();

        for _ in 0..20 {
            store.toggle_current(&uid).await.unwrap();
            store.toggle_current(&uid).await.unwrap();
        }

        store.toggle_current(&uid).await.unwrap();

        let timers = store.get_exportable_timers_by_tag(&uid).await.unwrap();

        assert_eq!(timers.len(), 20);
    }

    #[traced_test]
    #[tokio::test]
    async fn timer_update_end_time_success() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();

        // Start and stop a timer after sleeping for 2 seconds
        store.toggle_current(&uid).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let timer_id = store.toggle_current(&uid).await.unwrap();

        let timer = store.get_timer(timer_id).await.unwrap();
        let timer = timer.update_end_time().unwrap();

        assert_eq!(
            timer.end_time,
            timer
                .duration
                .and_then(|duration| Some(timer.start_time + duration))
        )
    }
}
