use std::{collections::HashMap, sync::OnceLock, time::Duration};

use anyhow::{anyhow, Error, Result};
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
            t.register_function("extract_timer_values", extract_timer_values());
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
    download_link: String,
    download_file_name: String,
}

impl Page {
    pub fn new(
        tag_name: String,
        timers: Vec<Timer>,
        download_link: String,
        download_file_name: String,
    ) -> Result<Self> {
        let timers: Result<Vec<Timer>, Error> =
            timers.into_iter().map(Timer::update_end_time).collect();
        let timers = timers?;

        Ok(Self {
            tag_name,
            timers,
            download_link,
            download_file_name,
        })
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
                let formatted_time = format_time(time, "%a, %F %H:%M")
                    .map_err(|err| tera::Error::call_function("to_human_date", err))?;

                Ok(to_value(formatted_time)?)
            }
            None => Err(tera::Error::call_function(
                "to_human_date",
                anyhow!("timestamp argument not found"),
            )),
        },
    )
}

pub fn format_time(time: i64, fmt_string: &str) -> Result<String> {
    match Local.timestamp_opt(time, 0) {
        chrono::LocalResult::None => Err(anyhow!("Unable to create DateTime object")),
        chrono::LocalResult::Single(time) => Ok(format!("{}", time.format(fmt_string))),
        chrono::LocalResult::Ambiguous(_, _) => {
            unreachable!("We shouldn't have ambiguious time")
        }
    }
}

/// Extracts the parts of time from a given timetamp
///
/// Mainly used to get the hours and minutes for a timer.
fn extract_timer_values() -> impl Function {
    Box::new(move |args: &HashMap<String, Value>| {
        let Some(time) = args.get("duration") else {
            return Err(tera::Error::call_function(
                "extract_timer_values",
                anyhow!("timestamp argument not found"),
            ));
        };

        let Some(part) = args.get("time_part") else {
            return Err(tera::Error::call_function(
                "extract_timer_values",
                anyhow!("time_part argument not found"),
            ));
        };

        let time = from_value::<i64>(time.clone())?;
        let part = from_value::<String>(part.clone())?;

        match part.as_str() {
            "minutes" => Ok(to_value(if time > 60 { time / 60 } else { 0 })?),
            "hours" => Ok(to_value(if time > 3600 { time / 3600 } else { 00 })?),
            _ => Err(tera::Error::call_function(
                "extract_timer_value",
                anyhow!("Unexpected time_part argument"),
            )),
        }
    })
}
