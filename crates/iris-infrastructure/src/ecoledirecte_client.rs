use std::env;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Local, NaiveDateTime, Utc};
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
    prenom: String,
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
    #[serde(default)]
    prenom: String,
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

#[derive(Clone)]
struct EdCredentials {
    username: String,
    password: String,
    student_id: String,
    child_name: String,
}

pub struct EcoleDirecteClient {
    accounts: Vec<EdCredentials>,
}

impl EcoleDirecteClient {
    pub fn from_env() -> Self {
        let mut accounts = Vec::new();

        let primary = EdCredentials {
            username: read_env_trimmed("IRIS_ECOLEDIRECTE_USER"),
            password: read_env_trimmed("IRIS_ECOLEDIRECTE_PASSWORD"),
            student_id: read_env_trimmed("IRIS_ECOLEDIRECTE_STUDENT_ID"),
            child_name: read_env_trimmed("IRIS_ECOLEDIRECTE_CHILD_NAME"),
        };
        if !primary.username.is_empty() && !primary.password.is_empty() {
            accounts.push(primary);
        }

        for idx in 2..=4 {
            let username = read_env_trimmed(&format!("IRIS_ECOLEDIRECTE_USER_{idx}"));
            let password = read_env_trimmed(&format!("IRIS_ECOLEDIRECTE_PASSWORD_{idx}"));
            if !username.is_empty() && !password.is_empty() {
                accounts.push(EdCredentials {
                    username,
                    password,
                    student_id: read_env_trimmed(&format!("IRIS_ECOLEDIRECTE_STUDENT_ID_{idx}")),
                    child_name: read_env_trimmed(&format!("IRIS_ECOLEDIRECTE_CHILD_NAME_{idx}")),
                });
            }
        }

        Self { accounts }
    }

    pub fn is_configured(&self) -> bool {
        !self.accounts.is_empty()
    }

    pub async fn fetch_week_timetable(&self) -> (Vec<OrgEvent>, EcoleDirecteStatus) {
        let now_ms = Utc::now().timestamp_millis();
        if !self.is_configured() {
            return (
                Vec::new(),
                EcoleDirecteStatus {
                    configured: false,
                    connected: false,
                    student_id: String::new(),
                    courses_count: 0,
                    last_error: Some(
                        "Identifiants non configurés (IRIS_ECOLEDIRECTE_USER / PASSWORD)"
                            .to_string(),
                    ),
                    updated_at_epoch_ms: now_ms,
                },
            );
        }

        let mut all_events = Vec::new();
        let mut all_sids = Vec::new();
        let mut errors = Vec::new();

        for cred in &self.accounts {
            match fetch_account_timetable(cred).await {
                Ok((mut events, sids)) => {
                    all_events.append(&mut events);
                    all_sids.extend(sids);
                }
                Err(err) => errors.push(err),
            }
        }

        let connected = !all_sids.is_empty() || errors.is_empty();
        let last_error = if errors.is_empty() {
            None
        } else {
            Some(errors.join(" | "))
        };
        let courses_count = all_events.len() as u32;

        (
            all_events,
            EcoleDirecteStatus {
                configured: true,
                connected,
                student_id: all_sids.join(", "),
                courses_count,
                last_error,
                updated_at_epoch_ms: Utc::now().timestamp_millis(),
            },
        )
    }
}

fn read_env_trimmed(key: &str) -> String {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

async fn fetch_account_timetable(
    cred: &EdCredentials,
) -> std::result::Result<(Vec<OrgEvent>, Vec<String>), String> {
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
        "identifiant": cred.username,
        "motdepasse": cred.password,
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

    let login: EdLoginResponse =
        serde_json::from_str(&login_text).map_err(|e| format!("Réponse login invalide: {e}"))?;

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

    let students = resolve_students(&cred.student_id, &cred.child_name, login.data.as_ref());
    let today = Local::now().date_naive();
    let end_day = today + ChronoDuration::days(6);

    let edt_payload = json!({
        "dateDebut": today.format("%Y-%m-%d").to_string(),
        "dateFin": end_day.format("%Y-%m-%d").to_string(),
        "avecTrous": false
    });
    let edt_body = format!("data={}", url_encode_component(&edt_payload.to_string()));

    let mut all_events = Vec::new();
    let mut sids = Vec::new();
    for (sid, child_name) in students {
        let edt_url = format!("{ED_BASE}/E/{sid}/emploidutemps.awp?verbe=get&v={ED_VERSION}");
        let edt_resp = http
            .post(&edt_url)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("X-Token", &login.token)
            .body(edt_body.clone())
            .send()
            .await
            .map_err(|e| format!("Requête emploi du temps ({sid}): {e}"))?;

        let edt_text = edt_resp
            .text()
            .await
            .map_err(|e| format!("Lecture emploi du temps ({sid}): {e}"))?;

        let mut events = parse_timetable_json_for_child(&edt_text, &sid, &child_name)?;
        all_events.append(&mut events);
        sids.push(sid);
    }

    Ok((all_events, sids))
}

impl Default for EcoleDirecteClient {
    fn default() -> Self {
        Self::from_env()
    }
}

fn resolve_students(
    configured_ids: &str,
    configured_child_name: &str,
    data: Option<&EdLoginData>,
) -> Vec<(String, String)> {
    let mut known_names = std::collections::HashMap::new();
    let mut discovered = Vec::new();

    if let Some(d) = data {
        for acc in &d.accounts {
            if let Some(prof) = &acc.profile {
                for eleve in &prof.eleves {
                    if eleve.id > 0 {
                        let id_str = eleve.id.to_string();
                        let name = eleve.prenom.trim().to_string();
                        known_names.insert(id_str.clone(), name.clone());
                        if !discovered.iter().any(|(id, _)| id == &id_str) {
                            discovered.push((id_str, name));
                        }
                    }
                }
            }
            if acc.account_type == "E" && acc.id > 0 {
                let id_str = acc.id.to_string();
                let name = acc.prenom.trim().to_string();
                known_names
                    .entry(id_str.clone())
                    .or_insert_with(|| name.clone());
                if !discovered.iter().any(|(id, _)| id == &id_str) {
                    discovered.push((id_str, name));
                }
            }
        }
    }

    let explicit_ids: Vec<String> = configured_ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();

    if !explicit_ids.is_empty() {
        return explicit_ids
            .into_iter()
            .map(|id| {
                let name = known_names
                    .get(&id)
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or_else(|| configured_child_name.trim().to_string());
                (id, name)
            })
            .collect();
    }

    if !discovered.is_empty() {
        return discovered;
    }

    vec![(
        DEFAULT_STUDENT_ID.to_string(),
        configured_child_name.trim().to_string(),
    )]
}

pub fn parse_timetable_json(body: &str) -> std::result::Result<Vec<OrgEvent>, String> {
    parse_timetable_json_for_child(body, "", "")
}

pub fn parse_timetable_json_for_child(
    body: &str,
    student_id: &str,
    child_name: &str,
) -> std::result::Result<Vec<OrgEvent>, String> {
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

    let child_prefix = if child_name.trim().is_empty() {
        String::new()
    } else {
        format!("[{}] ", child_name.trim())
    };
    let sid_tag = if student_id.trim().is_empty() {
        String::new()
    } else {
        format!("{}-", student_id.trim())
    };

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
            format!("[ANNULÉ] {child_prefix}{subject}")
        } else {
            format!("{child_prefix}{subject}")
        };
        if !course.prof.trim().is_empty() {
            title.push_str(&format!(" ({})", course.prof.trim()));
        }

        events.push(OrgEvent {
            id: format!(
                "ed-{sid_tag}{}-{}",
                course.id,
                start_dt.format("%Y%m%d%H%M")
            ),
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

    #[test]
    fn resolves_multiple_children_and_prefixes_name() {
        let login = EdLoginData {
            accounts: vec![EdAccount {
                id: 100,
                account_type: "1".to_string(),
                prenom: "Joseph".to_string(),
                profile: Some(EdProfile {
                    eleves: vec![
                        EdEleve {
                            id: 7858,
                            prenom: "Victor".to_string(),
                        },
                        EdEleve {
                            id: 7859,
                            prenom: "Alice".to_string(),
                        },
                    ],
                }),
            }],
        };

        let students = resolve_students("", "", Some(&login));
        assert_eq!(
            students,
            vec![
                ("7858".to_string(), "Victor".to_string()),
                ("7859".to_string(), "Alice".to_string()),
            ]
        );

        let json = r#"{
          "code": 200,
          "message": "",
          "data": [
            {
              "id": 201,
              "text": "FRANCAIS",
              "matiere": "Français",
              "prof": "",
              "salle": "C12",
              "start_date": "2026-11-16 09:00",
              "end_date": "2026-11-16 10:00",
              "isAnnule": false
            }
          ]
        }"#;
        let parsed = parse_timetable_json_for_child(json, "7859", "Alice");
        assert!(parsed.is_ok());
        if let Ok(events) = parsed {
            assert_eq!(events[0].id, "ed-7859-201-202611160900");
            assert_eq!(events[0].title, "[Alice] Français");
        }
    }
}
