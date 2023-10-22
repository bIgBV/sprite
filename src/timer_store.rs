use std::{
    collections::HashMap,
    env,
    fmt::Display,
    hash::Hash,
    time::{Duration, SystemTime},
};

use anyhow::Result;

use serde::Serialize;
use sqlx::SqlitePool;
use tracing::{debug, error, info, instrument};

use crate::uid::TagId;

#[derive(Debug, Clone)]
pub(crate) struct DataStore {
    pool: SqlitePool,
}

/// A Timer object
#[derive(Debug, sqlx::FromRow, Default, Serialize, PartialEq, Eq)]
#[sqlx]
pub struct Timer {
    /// The ID of this timer
    id: i64,

    /// The TagId this timer is associated with
    pub(crate) unique_id: String,

    /// The project this timer is associated with
    pub(crate) project_id: i64,

    /// When the timer was started
    pub(crate) start_time: i64,

    /// If this is the current timer associated with the [Timer::unique_id]
    pub(crate) is_current: bool,

    /// The duration for which this timer lasted.
    ///
    /// This value is only valid for timers for which `is_current` == false
    #[sqlx(default)]
    pub(crate) duration: i64,
}

#[derive(Debug)]
enum IsCurrent {
    Yes = 1,
    No = 0,
}

#[derive(Debug, sqlx::FromRow, Default, Serialize, PartialEq, Eq, Hash)]
#[sqlx]
pub struct Project {
    /// The ID of the Project
    pub id: i64,

    /// The name of the Project
    pub name: String,

    /// Whether or not this is the current project for the given timer tag
    pub is_current: bool,

    // The TagId this timer is associated with
    pub unique_id: String,
}

impl Display for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Timer {
    pub fn end_time(&self) -> i64 {
        self.start_time + self.duration
    }
}

impl DataStore {
    pub(crate) async fn new() -> Result<Self> {
        let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(DataStore { pool })
    }

    #[cfg(test)]
    async fn new_test(pool: SqlitePool) -> Result<Self> {
        Ok(DataStore { pool })
    }

    /// Toggles the current timer for the given UID
    #[instrument(skip(self))]
    pub async fn toggle_current(&self, uid: &TagId) -> Result<i64> {
        if let Ok(mut timer) = self.current_timer(uid).await {
            // We already have an existing timer
            let timer_id = timer.id;
            debug!(?timer, "Ending current timer");
            let timer_duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?
                - Duration::from_secs(timer.start_time.try_into()?);
            timer.duration = timer_duration.as_secs().try_into()?;

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

    #[instrument(skip(self))]
    /// Get the current project associated with the [`TagId`][crate::uid::TagId]
    ///
    /// Every project is associated with a **single** [`TagId`][crate::uid::TagId]
    async fn current_project(&self, uid: &TagId) -> Result<Project> {
        let tag_id = uid.as_ref();
        info!(tag_id, "Getting current project");

        let result = sqlx::query_as!(
            Project,
            r#"
SELECT * FROM PROJECTS
WHERE unique_id = ?1 AND is_current = ?2"#,
            tag_id,
            IsCurrent::Yes as i64
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    /// Gets all projects associated with [`TagId`][crate::uid::TagId]
    #[instrument(skip(self))]
    async fn get_projects(&self, uid: &TagId) -> Result<Vec<Project>> {
        let tag_id = uid.as_ref();
        info!(tag_id, "Getting projects");
        let result = sqlx::query_as!(
            Project,
            "SELECT * FROM PROJECTS WHERE unique_id = ?1",
            tag_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(result)
    }

    /// Creates a new project with the associated tag.
    ///
    /// If a project already exists, it ensures that the `is_current` status is handled properly.
    #[instrument(skip(self))]
    pub async fn create_project(&self, uid: &TagId, project_name: &str) -> Result<i64> {
        let tag_id = uid.as_ref();
        info!(tag_id, "Creating new project");

        // Update the existing current project if necessary
        match self.current_project(uid).await {
            Ok(p) => {
                sqlx::query!(
                    r#"
UPDATE projects
SET is_current = ?1
WHERE id = ?2;
                    "#,
                    IsCurrent::No as i64,
                    p.id
                )
                .execute(&self.pool)
                .await?;
            }
            Err(_) => {}
        };

        let id = sqlx::query!(
            r#"
INSERT INTO PROJECTS (UNIQUE_ID, IS_CURRENT, NAME)
VALUES (?1, ?2, ?3)"#,
            tag_id,
            IsCurrent::Yes as i64,
            project_name
        )
        .execute(&self.pool)
        .await?
        .last_insert_rowid();

        Ok(id)
    }

    #[cfg(test)]
    async fn get_timer(&self, timer_id: i64) -> Result<Timer> {
        Ok(sqlx::query_as!(
            Timer,
            r#"
SELECT * FROM TIMERS
WHERE id = ?1"#,
            timer_id,
        )
        .fetch_one(&self.pool)
        .await?)
    }

    /// Returns a map of projects->timers associated with given [`TagId`][crate::uid::TagId]
    pub(crate) async fn projects_by_tag(
        &self,
        timer_tag: &TagId,
    ) -> Result<HashMap<Project, Vec<Timer>>> {
        let tag = timer_tag.as_ref();
        info!(tag, "Generating project->timer map");
        struct JoinResult {
            project_name: String,
            project_id: i64,
            unique_id: String,
            project_is_current: bool,
            timer_id: i64,
            start_time: i64,
            timer_is_current: bool,
            duration: i64,
        }

        let result = sqlx::query_as!(
            JoinResult,
            r#"
SELECT 
    p.id AS project_id,
    p.name AS project_name, 
    p.unique_id AS unique_id, 
    p.is_current AS project_is_current, 
    t.id AS timer_id,
    t.start_time AS start_time, 
    t.is_current AS timer_is_current, 
    t.duration AS duration 
FROM projects p 
INNER JOIN timers t
    ON p.id = t.project_id
WHERE
    p.unique_id = ?1;
            "#,
            tag
        )
        .fetch_all(&self.pool)
        .await?;

        let mut map = HashMap::new();

        for row in result {
            let project = Project {
                name: row.project_name,
                id: row.project_id,
                is_current: row.project_is_current,
                unique_id: row.unique_id.clone(),
            };

            let timer = Timer {
                id: row.timer_id,
                unique_id: row.unique_id,
                project_id: project.id,
                start_time: row.start_time,
                is_current: row.timer_is_current,
                duration: row.duration,
            };
            (map.entry(project).or_insert_with(|| vec![])).push(timer)
        }

        Ok(map)
    }

    #[instrument(skip(self))]
    pub(crate) async fn get_exportable_timers_by_tag(
        &self,
        timer_tag: &TagId,
    ) -> Result<Vec<Timer>> {
        let tag = timer_tag.as_ref();
        info!(tag, "Exporting timers");

        let current_project = self.current_project(timer_tag).await?;

        let result = sqlx::query_as::<sqlx::Sqlite, Timer>(
            r#"
SELECT * FROM TIMERS
WHERE unique_id = ?1 AND IS_CURRENT = 0 AND PROJECT_ID = ?2
ORDER BY start_time DESC
            "#,
        )
        .bind(tag)
        .bind(current_project.id)
        .fetch_all(&self.pool)
        .await?;

        Ok(result)
    }

    /// Creates a new timer with the start time set to the unix epoch in UTC
    ///
    /// If the current project does not exist for the given
    /// [`TagId`][crate::uid::TagId] a new project is created.
    #[instrument(skip(self))]
    async fn create_timer(&self, uid: &TagId) -> Result<i64> {
        let tag_id = uid.as_ref();
        info!(tag_id, "Creating a new timer");

        let current_project = match self.current_project(uid).await {
            Ok(p) => p,
            Err(_) => {
                debug!(tag_id, "No current project found, creating a default");
                let _ = self.create_project(uid, "new-project").await?;
                self.current_project(uid).await?
            }
        };

        let start_epoch = chrono::Utc::now().timestamp();

        let id = sqlx::query!(
            r#"
INSERT INTO TIMERS (UNIQUE_ID, IS_CURRENT, START_TIME, PROJECT_ID)
VALUES (?1, ?2, ?3, ?4)"#,
            tag_id,
            IsCurrent::Yes as i64,
            start_epoch,
            current_project.id
        )
        .execute(&self.pool)
        .await?
        .last_insert_rowid();

        Ok(id)
    }

    #[instrument(skip_all)]
    async fn update_timer(&self, timer: Timer) -> Result<bool> {
        info!(timer = timer.id, "Updating timer");
        let rows = sqlx::query!(
            r#"
UPDATE TIMERS
SET is_current = ?1, duration = ?2
WHERE id = ?3
            "#,
            IsCurrent::No as i64,
            timer.duration,
            timer.id,
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        Ok(rows == 1)
    }

    #[instrument(skip(self))]
    async fn current_timer(&self, uid: &TagId) -> anyhow::Result<Timer> {
        let tag_id = uid.as_ref();
        info!(tag_id, "Fetching current timer");
        Ok(sqlx::query_as!(
            Timer,
            r#"
SELECT * FROM TIMERS
WHERE unique_id = ?1 AND is_current = ?2"#,
            tag_id,
            IsCurrent::Yes as i64
        )
        .fetch_one(&self.pool)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use pretty_assertions::assert_eq;
    use sqlx::sqlite::SqliteConnectOptions;
    use tracing_test::traced_test;

    async fn setup() -> Result<DataStore> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?;
        let pool = SqlitePool::connect_with(options).await?;

        sqlx::migrate!().run(&pool).await?;
        let store = DataStore::new_test(pool).await?;
        Ok(store)
    }

    #[traced_test]
    #[tokio::test]
    async fn toggle_create_when_timer_does_not_exist() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();
        store.create_project(&uid, "test-project").await.unwrap();

        let result = store.toggle_current(&uid).await.unwrap();

        assert_eq!(result, 1);
    }

    #[traced_test]
    #[tokio::test]
    async fn toggle_end_timer_when_current_already_exists() {
        let uid = TagId::new("test-tag").unwrap();
        let store = setup().await.unwrap();
        store.create_project(&uid, "test-project").await.unwrap();

        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        let timer = store.get_timer(result).await.unwrap();

        assert!(!timer.is_current, "Timer hasn't been turned off");

        assert!(timer.duration >= 2);
        assert!(!timer.is_current)
    }

    #[traced_test]
    #[tokio::test]
    async fn toggle_timer_creates_new_current_timer() {
        let uid = TagId::new("test-tag").unwrap();
        let store = setup().await.unwrap();
        store.create_project(&uid, "test-project").await.unwrap();

        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let result = store.toggle_current(&uid).await.unwrap();
        assert_eq!(result, 1);

        let timer = store.get_timer(result).await.unwrap();

        assert!(!timer.is_current, "Timer hasn't been turned off");

        assert!(timer.duration >= 2);
        assert!(!timer.is_current);

        let timer_id = store.toggle_current(&uid).await.unwrap();
        assert_eq!(timer_id, 2);
    }

    #[traced_test]
    #[tokio::test]
    async fn projects_by_tag_success() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();
        store.create_project(&uid, "test-project").await.unwrap();

        for _ in 0..20 {
            store.toggle_current(&uid).await.unwrap();
            store.toggle_current(&uid).await.unwrap();
        }

        let timers = store.projects_by_tag(&uid).await.unwrap();

        assert_eq!(timers.values().len(), 20);
    }

    #[traced_test]
    #[tokio::test]
    async fn get_exportable_timers_by_tag_returns_only_complete_timers() {
        let store = setup().await.unwrap();
        let uid = TagId::new("test-tag").unwrap();
        store.create_project(&uid, "test-project").await.unwrap();

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
        store.create_project(&uid, "test-project").await.unwrap();

        // Start and stop a timer after sleeping for 2 seconds
        store.toggle_current(&uid).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let timer_id = store.toggle_current(&uid).await.unwrap();

        let timer = store.get_timer(timer_id).await.unwrap();

        assert_eq!(timer.end_time(), timer.duration + timer.start_time)
    }
}
