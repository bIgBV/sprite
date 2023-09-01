use anyhow::Result;

pub fn load_env() -> Result<()> {
    if cfg!(debug_assertions) {
        dotenvy::dotenv()?;
    }

    Ok(())
}
