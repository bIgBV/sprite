use std::sync::OnceLock;

use anyhow::Result;
use axum::http::header::TE;
use tera::{Context, Tera};

use crate::timer_store::Timer;

pub static TEMPLATES: OnceLock<Tera> = OnceLock::new();

pub fn init_templates() {
    TEMPLATES.get_or_init(|| match Tera::new("assets/html/**/*.html") {
        Ok(t) => t,
        Err(e) => {
            println!("Error parsing templates: {}", e);
            ::std::process::abort();
        }
    });
}

pub fn render_template(timers: &[Timer]) -> Result<String> {
    TEMPLATES
        .get()
        .and_then(|tera| tera.render("index.html", &Context::new()).ok())
        .ok_or(anyhow::anyhow!("Unable to render index template"))
}
