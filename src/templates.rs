use std::sync::OnceLock;

use anyhow::Result;
use serde::Serialize;
use tera::{Context, Tera};
use tracing::{debug, error, instrument, trace};

use crate::timer_store::Timer;

pub static TEMPLATES: OnceLock<Tera> = OnceLock::new();

pub fn init_templates() {
    TEMPLATES.get_or_init(|| match Tera::new("assets/html/**/*.html") {
        Ok(t) => {
            trace!(?t.templates, "loaded templates");
            t
        }
        Err(e) => {
            println!("Error parsing templates: {}", e);
            ::std::process::abort();
        }
    });
}

#[derive(Debug, Serialize)]
pub struct Page {
    tag_name: String,
    timers: Vec<Timer>,
}

impl Page {
    pub fn new(tag_name: String, timers: Vec<Timer>) -> Self {
        Self { tag_name, timers }
    }
}

#[instrument(skip_all)]
pub fn render_timers(page: Page) -> Result<String> {
    let Some(tera) = TEMPLATES.get() else {
        return Err(anyhow::anyhow!("Unable to render index template"));
    };

    debug!(
        "Rendering {} timers for {} tag",
        page.timers.len(),
        page.tag_name
    );

    // #[derive(Debug, Serialize)]
    // struct Timers<'a>(&'a [Timer]);

    let mut context = Context::new();
    context.insert("page", &page);

    Ok(tera.render("index.html", &context).map_err(|err| {
        error!(%err, ?err.kind);
        err
    })?)
}
