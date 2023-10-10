use crate::{
    templates::{self, extract_timer},
    timer_store::Timer,
};
use anyhow::Result;
use csv::{Writer, WriterBuilder};
use serde::Serialize;

/// Serializes timers into a CSV writer
pub fn export_timers(timers: Vec<Timer>, timezone: String) -> Result<Writer<Vec<u8>>> {
    let data = vec![];
    let mut writer = WriterBuilder::new().from_writer(data);

    #[derive(Debug, Serialize)]
    struct ExportRecord {
        start_time: String,
        end_time: String,
        duration: String,
    }

    let timezone: chrono_tz::Tz = templates::from_render_timezone(timezone)?;

    for timer in timers {
        let timer = timer.update_end_time()?;

        let duration = if timer.duration.is_some() {
            let duration = timer.duration.expect("We already checked");
            format!(
                "{}:{}",
                extract_timer(templates::TimerPart::Hour, duration)?,
                extract_timer(templates::TimerPart::Min, duration)?
            )
        } else {
            String::new()
        };

        let export_timer = ExportRecord {
            start_time: templates::format_time(timer.start_time, timezone, "%F %H:%M")?,
            end_time: timer
                .end_time
                .and_then(|time| templates::format_time(time, timezone, "%F %H:%M").ok())
                .unwrap_or(String::new()),
            duration, // convert to minutes
        };
        writer.serialize(export_timer)?;
    }

    writer.flush()?;
    Ok(writer)
}
