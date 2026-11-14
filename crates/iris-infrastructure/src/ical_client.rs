use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use iris_core::{IrisError, OrgEvent, Result};

pub struct IcalClient {
    http: reqwest::Client,
}

impl IcalClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    pub async fn fetch_week_events(&self, ical_url: &str) -> Result<Vec<OrgEvent>> {
        let trimmed = ical_url.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let body = self
            .http
            .get(trimmed)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("ical fetch: {e}")))?
            .error_for_status()
            .map_err(|e| IrisError::Persistence(format!("ical status: {e}")))?
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("ical text: {e}")))?;

        let today = Local::now().date_naive();
        let end = today + Duration::days(6);
        Ok(parse_ics_events(&body, today, end))
    }
}

impl Default for IcalClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_ics_events(raw: &str, range_start: NaiveDate, range_end: NaiveDate) -> Vec<OrgEvent> {
    let unfolded = unfold_ics_lines(raw);
    let mut events = Vec::new();
    let mut in_event = false;
    let mut uid = String::new();
    let mut summary = String::new();
    let mut dtstart = String::new();
    let mut dtend = String::new();
    let mut location = String::new();

    for line in unfolded {
        if line == "BEGIN:VEVENT" {
            in_event = true;
            uid.clear();
            summary.clear();
            dtstart.clear();
            dtend.clear();
            location.clear();
            continue;
        }
        if line == "END:VEVENT" {
            if in_event && !summary.is_empty() {
                if let Some((date, start_time)) = parse_ics_datetime(&dtstart) {
                    if date >= range_start && date <= range_end {
                        let end_time = parse_ics_datetime(&dtend)
                            .map(|(_, t)| t)
                            .unwrap_or_default();
                        let id = if uid.is_empty() {
                            format!("ical-{date}-{summary}")
                        } else {
                            uid.clone()
                        };
                        events.push(OrgEvent {
                            id,
                            title: summary.clone(),
                            date: date.format("%Y-%m-%d").to_string(),
                            start_time,
                            end_time,
                            location: location.clone(),
                            source: "ical".to_string(),
                        });
                    }
                }
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        if let Some((key_raw, val)) = line.split_once(':') {
            let key = key_raw.split(';').next().unwrap_or("");
            match key {
                "UID" => uid = val.trim().to_string(),
                "SUMMARY" => summary = unescape_ics_text(val.trim()),
                "DTSTART" => dtstart = val.trim().to_string(),
                "DTEND" => dtend = val.trim().to_string(),
                "LOCATION" => location = unescape_ics_text(val.trim()),
                _ => {}
            }
        }
    }

    events.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| a.start_time.cmp(&b.start_time))
    });
    events
}

fn unfold_ics_lines(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in raw.lines() {
        let clean = line.trim_end_matches('\r');
        if let Some(rest) = clean.strip_prefix(' ').or_else(|| clean.strip_prefix('\t')) {
            if let Some(last) = out.last_mut() {
                last.push_str(rest);
            }
        } else {
            out.push(clean.to_string());
        }
    }
    out
}

fn parse_ics_datetime(raw: &str) -> Option<(NaiveDate, String)> {
    let clean = raw.trim().trim_end_matches('Z');
    if clean.len() == 8 {
        let date = NaiveDate::parse_from_str(clean, "%Y%m%d").ok()?;
        return Some((date, "Journée".to_string()));
    }
    if clean.len() >= 15 {
        let dt = NaiveDateTime::parse_from_str(&clean[..15], "%Y%m%dT%H%M%S").ok()?;
        return Some((dt.date(), dt.format("%H:%M").to_string()));
    }
    None
}

fn unescape_ics_text(s: &str) -> String {
    s.replace("\\n", " ")
        .replace("\\,", ",")
        .replace("\\;", ";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ics_events_within_week_range() {
        let sample = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:evt-1
DTSTART:20261114T093000
DTEND:20261114T103000
SUMMARY:Point Architecture Iris
LOCATION:Lyon
END:VEVENT
BEGIN:VEVENT
UID:evt-old
DTSTART:20261101T090000
SUMMARY:Ancien événement
END:VEVENT
END:VCALENDAR";

        let start = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap_or_default();
        let end = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap_or_default();
        let events = parse_ics_events(sample, start, end);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Point Architecture Iris");
        assert_eq!(events[0].start_time, "09:30");
        assert_eq!(events[0].end_time, "10:30");
        assert_eq!(events[0].location, "Lyon");
    }
}
