use std::env;

use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug)]
pub struct TimerStore {
    pool: SqlitePool,
}

impl TimerStore {
    async fn new() -> Result<Self> {
        let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;
        Ok(TimerStore { pool })
    }

    fn create_timer() -> Result<()> {
        Ok(())
    }
}
