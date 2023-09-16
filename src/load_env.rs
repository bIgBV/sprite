use std::env;

use anyhow::Result;

pub fn load_env() -> Result<()> {
    if cfg!(debug_assertions) {
        dotenvy::dotenv()?;
    } else {
        env::set_var("URI_BASE", "https://sprite.fly.dev/");
    }

    Ok(())
}
