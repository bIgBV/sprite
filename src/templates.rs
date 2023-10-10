use std::{collections::HashMap, sync::OnceLock};

use anyhow::{anyhow, Error, Result};
use chrono::TimeZone;
use serde::Serialize;
use tera::{from_value, to_value, Context, Function, Tera, Value};
use tracing::{debug, error, instrument, trace};

use crate::{timer_store::Timer, uid::TagId, uri_base};

pub static DEFAULT_TIMEZONES: [chrono_tz::Tz; 4] = [
    chrono_tz::US::Pacific,
    chrono_tz::US::Mountain,
    chrono_tz::US::Central,
    chrono_tz::US::Eastern,
];
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
pub struct MainPage {
    current_timezone: String,
    tag_name: String,
    timezones: Vec<String>,
    uri_base: String,
    projects: Vec<Project>,
}

/// Data structure for holding information related to a project
#[derive(Debug, Serialize)]
struct Project {
    name: String,
    timers: Vec<Timer>,
    download_link: String,
    download_file_name: String,
}

impl MainPage {
    pub fn new(tag_name: String, timers: Vec<Timer>, timezone: Option<String>) -> Result<Self> {
        let timers: Result<Vec<Timer>, Error> =
            timers.into_iter().map(Timer::update_end_time).collect();
        let timers = timers?;

        let timezone: chrono_tz::Tz = if let Some(timezone) = timezone {
            from_render_timezone(timezone)?
        } else {
            chrono_tz::US::Pacific
        };

        let projects =
            timers_to_project(timers, timezone.clone()).collect::<Result<Vec<Project>>>()?;

        let timezones = DEFAULT_TIMEZONES
            .iter()
            .filter(|tz| **tz != timezone)
            .map(|tz| format!("{}", to_render_timezone(tz)))
            .collect();

        Ok(Self {
            tag_name,
            current_timezone: format!("{}", to_render_timezone(&timezone)),
            timezones,
            uri_base: uri_base(),
            projects,
        })
    }
}

fn timers_to_project(
    timers: Vec<Timer>,
    timezone: chrono_tz::Tz,
) -> impl Iterator<Item = Result<Project>> {
    let mut project_map = HashMap::new();

    for timer in timers {
        project_map
            .entry(timer.project.clone())
            .and_modify(|val: &mut Vec<Timer>| val.push(timer))
            .or_insert(Vec::new());
    }

    project_map.into_iter().map(move |(key, val)| {
        if val.len() == 0 {
            return Ok(Project {
                name: key,
                timers: val,
                download_link: String::new(),
                download_file_name: String::new(),
            });
        }

        let tag = val
            .iter()
            .take(1)
            .map(|t| t.unique_id.as_ref())
            .next()
            .expect("There should be at least one timer");

        let (file_name, link) = download_information(&key, &tag, &timezone);

        Ok(Project {
            name: key,
            timers: val,
            download_link: link,
            download_file_name: file_name,
        })
    })
}

fn download_information(project: &str, tag: &str, timezone: &chrono_tz::Tz) -> (String, String) {
    let file_name = format!("{}.csv", project);
    let link = format!(
        "{}/export/{}/{}/{}",
        uri_base(),
        tag,
        file_name,
        to_render_timezone(timezone)
    );

    (file_name, link)
}

#[instrument(skip(timers))]
pub fn render_timers(tag: TagId, timezone: Option<String>, timers: Vec<Timer>) -> Result<String> {
    let Some(tera) = TEMPLATES.get() else {
        return Err(anyhow::anyhow!("Unable to render index template"));
    };

    let page = MainPage::new(tag.as_ref().to_string(), timers, timezone)?;

    debug!("Rendering timers for {} tag", page.tag_name);

    let mut context = Context::new();
    context.insert("page", &page);

    Ok(tera.render("index.html", &context).map_err(|err| {
        error!(%err, ?err.kind);
        err
    })?)
}

/// Convert US/<Zone> -> US-Zone to ensure a subroute isn't created
pub fn to_render_timezone(timezone: &chrono_tz::Tz) -> String {
    let zone = format!("{}", timezone);
    zone.replace("/", "-")
}

/// Convert rendered US-Zone -> US/Zone
pub fn from_render_timezone(timezone: String) -> Result<chrono_tz::Tz> {
    let zone = timezone.replace("-", "/");
    zone.parse()
        .map_err(|err| anyhow!("Unable to parse timezone: {}", err))
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
        let rendered_timezone = from_value::<String>(timezone.clone())?;
        let timezone: chrono_tz::Tz = from_render_timezone(rendered_timezone).map_err(|_| {
            tera::Error::call_function("to_human_date", anyhow!("Unable to convert timezone"))
        })?;
        let formatted_time = format_time(time, timezone, "%a, %F %H:%M")
            .map_err(|err| tera::Error::call_function("to_human_date", err))?;

        Ok(to_value(formatted_time)?)
    })
}

#[instrument]
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
        let part = match from_value::<String>(part.clone())?.as_str() {
            "hours" => TimerPart::Hour,
            "minutes" => TimerPart::Min,
            _ => panic!("Unexpected argument"),
        };

        let Ok(part) = extract_timer(part, time) else {
            return Err(tera::Error::call_function(
                "extract_timer_value",
                anyhow!("Unexpected time_part argument"),
            ));
        };

        Ok(to_value(part)?)
    })
}

pub enum TimerPart {
    Hour,
    Min,
}

/// Extracts the minute and hour parts of the duration.
///
/// Duration is stored in minute resolution
pub fn extract_timer(part: TimerPart, time: i64) -> Result<i64> {
    let minute = 60;
    let hour = 60 * minute;
    match part {
        // Round minutes to the nearest minute
        TimerPart::Min => {
            // Round down to only the minutes portion
            let minutes = if time > hour { time % hour } else { time };

            Ok(if minutes > minute {
                minutes / minute
            } else {
                0
            })
        }
        TimerPart::Hour => Ok(if time > hour { time / hour } else { 0 }),
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn extract_timer_under_hour() {
        let result = extract_timer(TimerPart::Min, 45);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        let result = extract_timer(TimerPart::Min, 45 * 60);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 45);
    }

    #[test]
    fn extract_timer_over_hour() {
        let time = ((2 * 60) + 35) * 60;
        let result = extract_timer(TimerPart::Min, time);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 35);

        let result = extract_timer(TimerPart::Hour, time);
        assert_eq!(result.unwrap(), 2);
    }

    #[test]
    fn extract_timer_long_duration() {
        let time = 74242;

        let result = extract_timer(TimerPart::Min, time);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 37);

        let result = extract_timer(TimerPart::Hour, time);
        assert_eq!(result.unwrap(), 20);
    }
}
