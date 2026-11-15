use std::env;
use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDateTime, Utc};
use iris_core::{EcoleDirecteStatus, OrgEvent};
use serde::Deserialize;
use serde_json::{json, Value};

const ED_BASE: &str = "https://api.ecoledirecte.com/v3";
const ED_VERSION: &str = "4.65.0";
const DEFAULT_STUDENT_ID: &str = "7858";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Deserialize)]
struct EdLoginResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    data: Option<EdLoginData>,
}

#[derive(Deserialize)]
struct EdLoginData {
    #[serde(default)]
    accounts: Vec<EdAccount>,
}

#[derive(Deserialize)]
struct EdAccount {
    #[serde(default)]
    id: i64,
    #[serde(rename = "typeCompte", default)]
    account_type: String,
    #[serde(default)]
    profile: Option<EdProfile>,
}

#[derive(Deserialize)]
struct EdProfile {
    #[serde(default)]
    eleves: Vec<EdEleve>,
}

#[derive(Deserialize)]
struct EdEleve {
    #[serde(default)]
    id: i64,
}

#[derive(Deserialize)]
struct EdTimetableResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Vec<EdCourse>,
}

#[derive(Deserialize)]
struct EdCourse {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    matiere: String,
    #[serde(default)]
    prof: String,
    #[serde(default)]
    salle: String,
    #[serde(default)]
    start_date: String,
    #[serde(default)]
    end_date: String,
    #[serde(rename = "isAnnule", default)]
    is_annule: bool,
}

pub struct EcoleDirecteClient {
    username: String,
    password: String,
    student_id: String,
}

impl EcoleDirecteClient {
    pub fn from_env() -> Self {
        let username = env::var("IRIS_ECOLEDIRECTE_USER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_default();
        let password = env::var("IRIS_ECOLEDIRECTE_PASSWORD")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_default();
        let student_id = env::var("IRIS_ECOLEDIRECTE_STUDENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_STUDENT_ID.to_string());

        Self {
            username,
            password,
            student_id,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.username.trim().is_empty() && !self.password.trim().is_empty()
    }

    pub async fn fetch_week_timetable(&self) -> (Vec<OrgEvent>, EcoleDirecteStatus) {
        let now_ms = Utc::now().timestamp_millis();
        if !self.is_configured() {
            return (
                Vec::new(),
                EcoleDirecteStatus {
                    configured: false,
                    connected: false,
                    student_id: self.student_id.clone(),
                    courses_count: 0,
                    last_error: Some(
                        "Identifiants non configurés (IRIS_ECOLEDIRECTE_USER / PASSWORD)"
                            .to_string(),
                    ),
                    updated_at_epoch_ms: now_ms,
                },
            );
        }

        match self.fetch_internal().await {
            Ok((events, sid)) => {
                let count = events.len() as u32;
                (
                    events,
                    EcoleDirecteStatus {
                        configured: true,
                        connected: true,
                        student_id: sid,
                        courses_count: count,
                        last_error: None,
                        updated_at_epoch_ms: Utc::now().timestamp_millis(),
                    },
                )
            }
            Err(err) => (
                Vec::new(),
                EcoleDirecteStatus {
                    configured: true,
                    connected: false,
                    student_id: self.student_id.clone(),
                    courses_count: 0,
                    last_error: Some(err),
                    updated_at_epoch_ms: Utc::now().timestamp_millis(),
                },
            ),
        }
    }

    async fn fetch_internal(&self) -> std::result::Result<(Vec<OrgEvent>, String), String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|e| format!("Client HTTP ÉcoleDirecte: {e}"))?;

        let gtk_url = format!("{ED_BASE}/login.awp?gtk=1&v={ED_VERSION}");
        let gtk_resp = http
            .get(&gtk_url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|e| format!("Handshake GTK échoué: {e}"))?;

        let mut gtk_token = String::new();
        let mut cookie_pairs = Vec::new();
        for val in gtk_resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(raw) = val.to_str() {
                if let Some(pair) = raw.split(';').next() {
                    let trimmed = pair.trim();
                    if let Some(v) = trimmed.strip_prefix("GTK=") {
                        gtk_token = v.to_string();
                    }
                    cookie_pairs.push(trimmed.to_string());
                }
            }
        }
        let cookie_header = cookie_pairs.join("; ");

        let payload = json!({
            "identifiant": self.username,
            "motdepasse": self.password,
            "isReLogin": false,
            "uuid": ""
        });
        let form_body = format!("data={}", url_encode_component(&payload.to_string()));

        let login_url = format!("{ED_BASE}/login.awp?v={ED_VERSION}");
        let mut req = http
            .post(&login_url)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body);

        if !gtk_token.is_empty() {
            req = req.header("X-Gtk", &gtk_token);
        }
        if !cookie_header.is_empty() {
            req = req.header("Cookie", &cookie_header);
        }

        let login_resp = req
            .send()
            .await
            .map_err(|e| format!("Requête login ÉcoleDirecte: {e}"))?;

        let login_text = login_resp
            .text()
            .await
            .map_err(|e| format!("Lecture login ÉcoleDirecte: {e}"))?;

        let login: EdLoginResponse = serde_json::from_str(&login_text)
            .map_err(|e| format!("Réponse login invalide: {e}"))?;

        if login.code == 250 {
            return Err(
                "Double authentification QCM requise par ÉcoleDirecte sur cette IP".to_string(),
            );
        }
        if login.code != 200 || login.token.is_empty() {
            let msg = if login.message.is_empty() {
                format!("Code erreur {}", login.code)
            } else {
                login.message
            };
            return Err(msg);
        }

        let sid = resolve_student_id(&self.student_id, login.data.as_ref());
        let today = Local::now().date_naive();
        let days_since_mon = i64::from(today.weekday().num_days_from_monday());
        let monday = today - ChronoDuration::days(days_since_mon);
        let sunday = monday + ChronoDuration::days(6);

        let edt_payload = json!({
            "dateDebut": monday.format("%Y-%m-%d").to_string(),
            "dateFin": sunday.format("%Y-%m-%d").to_string(),
            "avecTrous": false
        });
        let edt_body = format!("data={}", url_encode_component(&edt_payload.to_string()));
        let edt_url = format!("{ED_BASE}/E/{sid}/emploidutemps.awp?verbe=get&v={ED_VERSION}");
        let edt_resp = http
            .post(&edt_url)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("X-Token", &login.token)
            .body(edt_body)
            .send()
            .await
            .map_err(|e| format!("Requête emploi du temps: {e}"))?;

        let edt_text = edt_resp
            .text()
            .await
            .map_err(|e| format!("Lecture emploi du temps: {e}"))?;

        let events = parse_timetable_json(&edt_text)?;
        Ok((events, sid))
    }
}

impl Default for EcoleDirecteClient {
    fn default() -> Self {
        Self::from_env()
    }
}

fn resolve_student_id(configured: &str, data: Option<&EdLoginData>) -> String {
    if !configured.trim().is_empty() {
        return configured.trim().to_string();
    }
    if let Some(d) = data {
        for acc in &d.accounts {
            if acc.account_type == "E" && acc.id > 0 {
                return acc.id.to_string();
            }
            if let Some(prof) = &acc.profile {
                if let Some(eleve) = prof.eleves.first() {
                    if eleve.id > 0 {
                        return eleve.id.to_string();
                    }
                }
            }
        }
    }
    DEFAULT_STUDENT_ID.to_string()
}

pub fn parse_timetable_json(body: &str) -> std::result::Result<Vec<OrgEvent>, String> {
    let val: Value =
        serde_json::from_str(body).map_err(|e| format!("JSON emploi du temps invalide: {e}"))?;
    let parsed: EdTimetableResponse = serde_json::from_value(val)
        .map_err(|e| format!("Structure emploi du temps invalide: {e}"))?;

    if parsed.code != 200 && parsed.code != 0 {
        return Err(if parsed.message.is_empty() {
            format!("Erreur emploi du temps ({})", parsed.code)
        } else {
            parsed.message
        });
    }

    let mut events = Vec::new();
    for course in parsed.data {
        let Ok(start_dt) =
            NaiveDateTime::parse_from_str(course.start_date.trim(), "%Y-%m-%d %H:%M")
        else {
            continue;
        };
        let end_time = NaiveDateTime::parse_from_str(course.end_date.trim(), "%Y-%m-%d %H:%M")
            .map(|dt| dt.format("%H:%M").to_string())
            .unwrap_or_default();

        let subject = if !course.matiere.trim().is_empty() {
            course.matiere.trim().to_string()
        } else {
            course.text.trim().to_string()
        };
        if subject.is_empty() {
            continue;
        }

        let mut title = if course.is_annule {
            format!("[ANNULÉ] {subject}")
        } else {
            subject
        };
        if !course.prof.trim().is_empty() {
            title.push_str(&format!(" ({})", course.prof.trim()));
        }

        events.push(OrgEvent {
            id: format!("ed-{}-{}", course.id, start_dt.format("%Y%m%d%H%M")),
            title,
            date: start_dt.format("%Y-%m-%d").to_string(),
            start_time: start_dt.format("%H:%M").to_string(),
            end_time,
            location: course.salle.trim().to_string(),
            source: "ecoledirecte".to_string(),
        });
    }
    Ok(events)
}

fn url_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ecoledirecte_timetable() {
        let json = r#"{
          "code": 200,
          "message": "",
          "data": [
            {
              "id": 101,
              "text": "MATHS",
              "matiere": "Mathématiques",
              "prof": "M. Dupont",
              "salle": "B204",
              "start_date": "2026-11-16 08:30",
              "end_date": "2026-11-16 09:25",
              "isAnnule": false
            },
            {
              "id": 102,
              "text": "HISTOIRE",
              "matiere": "Histoire-Géo",
              "prof": "Mme Martin",
              "salle": "A102",
              "start_date": "2026-11-16 10:30",
              "end_date": "2026-11-16 11:25",
              "isAnnule": true
            }
          ]
        }"#;

        let res = parse_timetable_json(json);
        assert!(res.is_ok());
        if let Ok(events) = res {
            assert_eq!(events.len(), 2);
            assert_eq!(events[0].date, "2026-11-16");
            assert_eq!(events[0].start_time, "08:30");
            assert_eq!(events[0].end_time, "09:25");
            assert_eq!(events[0].title, "Mathématiques (M. Dupont)");
            assert_eq!(events[0].location, "B204");
            assert_eq!(events[0].source, "ecoledirecte");
            assert!(events[1].title.starts_with("[ANNULÉ]"));
        }
    }
}
