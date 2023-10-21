use std::collections::HashMap;

use anyhow::{anyhow, Result};
use askama::Template;
use askama_axum;
use chrono::TimeZone;
use serde::Serialize;
use tracing::{debug, instrument};

use crate::{timer_store::Timer, uid::TagId, uri_base};

pub(crate) static DEFAULT_TIMEZONES: [chrono_tz::Tz; 4] = [
    chrono_tz::US::Pacific,
    chrono_tz::US::Mountain,
    chrono_tz::US::Central,
    chrono_tz::US::Eastern,
];

#[derive(Debug, Serialize, Template)]
#[template(path = "index.html")]
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
    pub(crate) fn new(
        tag_name: String,
        timers: Vec<Timer>,
        timezone: Option<String>,
    ) -> Result<Self> {
        let current_timezone: chrono_tz::Tz = if let Some(timezone) = timezone {
            from_render_timezone(&timezone)?
        } else {
            chrono_tz::US::Pacific
        };

        let projects = timers_to_project(timers, current_timezone.clone())
            .collect::<Result<Vec<Project>>>()?;

        let timezones = DEFAULT_TIMEZONES
            .iter()
            .filter(|tz| **tz != current_timezone)
            .map(|tz| format!("{}", to_render_timezone(tz)))
            .collect();

        Ok(Self {
            tag_name,
            current_timezone: format!("{}", to_render_timezone(&current_timezone)),
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
            .entry("dummy".to_string())
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
pub fn render_timers(
    tag: TagId,
    timezone: Option<String>,
    timers: Vec<Timer>,
) -> anyhow::Result<MainPage> {
    let page = MainPage::new(tag.as_ref().to_string(), timers, timezone)?;

    debug!("Rendering timers for {} tag", page.tag_name);
    Ok(page)
}

/// Convert US/<Zone> -> US-Zone to ensure a subroute isn't created
pub(crate) fn to_render_timezone(timezone: &chrono_tz::Tz) -> String {
    let zone = format!("{}", timezone);
    zone.replace("/", "-")
}

/// Convert rendered US-Zone -> US/Zone
pub(crate) fn from_render_timezone(timezone: &str) -> Result<chrono_tz::Tz> {
    let zone = timezone.replace("-", "/");
    zone.parse()
        .map_err(|err| anyhow!("Unable to parse timezone: {}", err))
}

mod filters {
    use std::{fmt::Display, num::ParseIntError};

    use super::TimerPart;

    pub fn to_human_date(timestamp: &i64, timezone: &str) -> ::askama::Result<String> {
        let timezone: chrono_tz::Tz = super::from_render_timezone(timezone)
            .map_err(|err| askama::Error::Custom(err.into()))?;
        let formatted_time = super::format_time(timestamp, timezone, "%a, %F %H:%M")
            .map_err(|err| askama::Error::Custom(err.into()))?;

        Ok(formatted_time)
    }

    /// Extracts the parts of time from a given timetamp
    ///
    /// Mainly used to get the hours and minutes for a timer.
    pub fn extract_timer_values<T: Display>(time: T, part: &str) -> ::askama::Result<String> {
        let time = time
            .to_string()
            .parse()
            .map_err(|err: ParseIntError| askama::Error::Custom(Box::new(err)))?; // Inference trips over itself without type
        let part = match part {
            "hours" => TimerPart::Hour,
            "minutes" => TimerPart::Min,
            _ => panic!("Unexpected argument"),
        };

        let Ok(part) = super::extract_timer(part, time) else {
            return Err(askama::Error::Custom("Unable to extract timer part".into()));
        };

        Ok(format!("{}", part))
    }
}

pub enum TimerPart {
    Hour,
    Min,
}

#[instrument]
pub fn format_time(time: &i64, timezone: chrono_tz::Tz, fmt_string: &str) -> Result<String> {
    match timezone.timestamp_opt(*time, 0) {
        chrono::LocalResult::None => Err(anyhow!("Unable to create DateTime object")),
        chrono::LocalResult::Single(time) => Ok(format!("{}", time.format(fmt_string))),
        chrono::LocalResult::Ambiguous(_, _) => {
            unreachable!("We shouldn't have ambiguious time")
        }
    }
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
