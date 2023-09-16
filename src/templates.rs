use std::{collections::HashMap, sync::OnceLock};

use anyhow::{anyhow, Error, Result};
use chrono::TimeZone;
use serde::Serialize;
use tera::{from_value, to_value, Context, Function, Tera, Value};
use tracing::{debug, error, instrument, trace};

use crate::timer_store::Timer;

pub static TEMPLATES: OnceLock<Tera> = OnceLock::new();

pub fn init_templates() {
    TEMPLATES.get_or_init(|| match Tera::new("assets/html/**/*.html") {
        Ok(mut t) => {
            trace!(?t.templates, "loaded templates");
            t.register_function("to_human_date", to_human_date());
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
    timezone: String,
    tag_name: String,
    timers: Vec<Timer>,
    download_link: String,
    download_file_name: String,
    timezones: Vec<String>,
}

impl Page {
    pub fn new(
        tag_name: String,
        timers: Vec<Timer>,
        download_link: String,
        download_file_name: String,
        timezone: Option<chrono_tz::Tz>,
    ) -> Result<Self> {
        let timers: Result<Vec<Timer>, Error> =
            timers.into_iter().map(Timer::update_end_time).collect();
        let timers = timers?;

        let timezone = timezone
            .or_else(|| Some(chrono_tz::US::Pacific))
            .expect("The universe hates me");

        Ok(Self {
            tag_name,
            timers,
            download_link,
            download_file_name,
            timezone: format!("{}", timezone),
            timezones: vec![
                format!("{}", chrono_tz::US::Mountain),
                format!("{}", chrono_tz::US::Central),
                format!("{}", chrono_tz::US::Eastern),
            ],
        })
    }
}

#[instrument]
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

fn to_human_date() -> impl Function {
    Box::new(move |args: &HashMap<String, Value>| {
        let Some(timestamp) = args.get("timestamp") else {
            return Err(tera::Error::call_function(
                "to_human_date",
                anyhow!("timestamp argument not found"),
            ));
        };

        let Some(timezone) = args.get("timezone") else {
            return Err(tera::Error::call_function(
                "to_human_date",
                anyhow!("timezone argument not found"),
            ));
        };
        let time = from_value::<i64>(timestamp.clone())?;
        let timezone: chrono_tz::Tz = from_value::<String>(timezone.clone())?.parse()?;
        let formatted_time = format_time(time, timezone, "%a, %F %H:%M")
            .map_err(|err| tera::Error::call_function("to_human_date", err))?;

        Ok(to_value(formatted_time)?)
    })
}

pub fn format_time(time: i64, timezone: chrono_tz::Tz, fmt_string: &str) -> Result<String> {
    match timezone.timestamp_opt(time, 0) {
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
