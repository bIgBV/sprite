use std::{collections::HashMap, sync::OnceLock};

use anyhow::{anyhow, Result};
use chrono::{Local, TimeZone};
use serde::Serialize;
use tera::{from_value, to_value, Context, Function, Tera, Value};
use tracing::{debug, error, instrument, trace};

use crate::timer_store::Timer;

pub static TEMPLATES: OnceLock<Tera> = OnceLock::new();

pub fn init_templates() {
    TEMPLATES.get_or_init(|| match Tera::new("assets/html/**/*.html") {
        Ok(mut t) => {
            trace!(?t.templates, "loaded templates");
            t.register_function("to_human_date", to_human_date(0, "timezone".to_string()));
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

// TODO: Register this method with tera specifying the timezone local timezone
fn to_human_date(_timestamp: i64, _timezone: String) -> impl Function {
    Box::new(
        move |args: &HashMap<String, Value>| match args.get("timestamp") {
            Some(val) => {
                let time = from_value::<i64>(val.clone())?;
                let formatted_time = match Local.timestamp_opt(time, 0) {
                    chrono::LocalResult::None => Err(tera::Error::call_function(
                        "to_human_date",
                        anyhow!("Unable to create DateTime object"),
                    )),
                    chrono::LocalResult::Single(time) => {
                        Ok(format!("{}", time.format("%a, %F %H:%M")))
                    }
                    chrono::LocalResult::Ambiguous(_, _) => {
                        unreachable!("We shouldn't have ambiguious time")
                    }
                };

                Ok(to_value(formatted_time?)?)
            }
            None => Err(tera::Error::call_function(
                "to_human_date",
                anyhow!("timestamp argument now found"),
            )),
        },
    )
}
