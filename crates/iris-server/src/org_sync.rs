use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use iris_application::IrisController;
use iris_core::{ActiveAlert, AlertSeverity};
use iris_infrastructure::{
    AlexaMcpClient, AmarakiClient, EcoleDirecteClient, EnderClient, GitHubClient,
    HomeAssistantClient, IcalClient, K8sClient, KusanagiClient, OpenMeteoClient, SlackClient,
};
use tracing::{debug, warn};

const POWER_OVERLOAD_THRESHOLD_W: i32 = 7000;
const POWER_CONFIRM_DURATION: Duration = Duration::from_secs(55);
const POWER_NOTIFY_COOLDOWN: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PowerAlertTransition {
    None,
    Trigger { consumption_w: i32, solar_w: i32 },
    Clear,
}

#[derive(Default)]
struct PowerOverloadMonitor {
    consecutive_high: u32,
    first_high_at: Option<Instant>,
    last_notified_at: Option<Instant>,
    alert_active: bool,
}

impl PowerOverloadMonitor {
    fn new() -> Self {
        Self::default()
    }

    fn evaluate(&mut self, consumption_w: i32, solar_w: i32, now: Instant) -> PowerAlertTransition {
        let overloaded_no_solar = consumption_w > POWER_OVERLOAD_THRESHOLD_W && solar_w <= 0;
        if !overloaded_no_solar {
            self.consecutive_high = 0;
            self.first_high_at = None;
            if self.alert_active {
                self.alert_active = false;
                return PowerAlertTransition::Clear;
            }
            return PowerAlertTransition::None;
        }

        self.consecutive_high = self.consecutive_high.saturating_add(1);
        let first_at = *self.first_high_at.get_or_insert(now);

        if self.consecutive_high >= 2 && now.duration_since(first_at) >= POWER_CONFIRM_DURATION {
            let cooldown_elapsed = self
                .last_notified_at
                .is_none_or(|prev| now.duration_since(prev) >= POWER_NOTIFY_COOLDOWN);
            if cooldown_elapsed {
                self.last_notified_at = Some(now);
                self.alert_active = true;
                return PowerAlertTransition::Trigger {
                    consumption_w,
                    solar_w,
                };
            }
        }

        PowerAlertTransition::None
    }
}

struct FastSyncClients<'a> {
    ha: &'a HomeAssistantClient,
    kusanagi: &'a KusanagiClient,
    slack: &'a SlackClient,
    amaraki: &'a AmarakiClient,
    alexa: &'a AlexaMcpClient,
    ender: &'a EnderClient,
}

pub fn spawn_org_sync_loop(controller: Arc<IrisController>) {
    let fast_ctrl = Arc::clone(&controller);
    tokio::spawn(async move {
        let ha = HomeAssistantClient::from_env();
        let kusanagi = KusanagiClient::from_env();
        let slack = SlackClient::from_env();
        let amaraki = AmarakiClient::from_env();
        let alexa = AlexaMcpClient::new();
        let ender = EnderClient::from_env();
        let mut power_monitor = PowerOverloadMonitor::new();
        loop {
            let clients = FastSyncClients {
                ha: &ha,
                kusanagi: &kusanagi,
                slack: &slack,
                amaraki: &amaraki,
                alexa: &alexa,
                ender: &ender,
            };
            refresh_fast_once(&fast_ctrl, &clients, Some(&mut power_monitor)).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    tokio::spawn(async move {
        let weather = OpenMeteoClient::new();
        let ical = IcalClient::new();
        let k8s = K8sClient::new();
        let github = GitHubClient::new();
        let alexa = AlexaMcpClient::new();
        let ecoledirecte = EcoleDirecteClient::from_env();
        loop {
            refresh_slow_once(
                &controller,
                &weather,
                &ical,
                &k8s,
                &github,
                &alexa,
                &ecoledirecte,
            )
            .await;
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

pub async fn refresh_org_now(controller: Arc<IrisController>) {
    let ha = HomeAssistantClient::from_env();
    let kusanagi = KusanagiClient::from_env();
    let slack = SlackClient::from_env();
    let amaraki = AmarakiClient::from_env();
    let ender = EnderClient::from_env();
    let weather = OpenMeteoClient::new();
    let ical = IcalClient::new();
    let k8s = K8sClient::new();
    let github = GitHubClient::new();
    let alexa = AlexaMcpClient::new();
    let ecoledirecte = EcoleDirecteClient::from_env();

    let fast_clients = FastSyncClients {
        ha: &ha,
        kusanagi: &kusanagi,
        slack: &slack,
        amaraki: &amaraki,
        alexa: &alexa,
        ender: &ender,
    };

    tokio::join!(
        refresh_fast_once(&controller, &fast_clients, None),
        refresh_slow_once(
            &controller,
            &weather,
            &ical,
            &k8s,
            &github,
            &alexa,
            &ecoledirecte
        ),
    );
}

async fn refresh_fast_once(
    controller: &IrisController,
    clients: &FastSyncClients<'_>,
    power_monitor: Option<&mut PowerOverloadMonitor>,
) {
    let ha_fut = async {
        if !clients.ha.is_configured() {
            return None;
        }
        match clients.ha.fetch_data().await {
            Ok(data) => Some(data),
            Err(err) => {
                warn!(%err, "Failed to fetch Home Assistant states");
                None
            }
        }
    };
    let kus_fut = async {
        match clients.kusanagi.fetch_infra().await {
            Ok(data) => Some(data),
            Err(err) => {
                debug!(%err, "Kusanagi infra endpoints unavailable");
                None
            }
        }
    };
    let slack_fut = async {
        if !clients.slack.is_configured() {
            return None;
        }
        match clients.slack.fetch_recent_messages().await {
            Ok(msgs) => Some(msgs),
            Err(err) => {
                debug!(%err, "Slack recent messages unavailable");
                None
            }
        }
    };
    let amaraki_fut = async {
        match clients.amaraki.fetch_reports().await {
            Ok(reports) => Some(reports),
            Err(err) => {
                debug!(%err, "Amaraki agent reports unavailable");
                None
            }
        }
    };
    let ender_fut = async {
        match clients.ender.fetch_status().await {
            Ok(status) => Some(status),
            Err(err) => {
                debug!(%err, "Ender 3D status unavailable");
                None
            }
        }
    };

    let (ha_data, kus_data, slack_msgs, amaraki_reports, ender_status) =
        tokio::join!(ha_fut, kus_fut, slack_fut, amaraki_fut, ender_fut);
    let power_sample = ha_data.as_ref().and_then(|d| {
        d.energy
            .home_consumption_w
            .map(|c| (c, d.energy.solar_now_w.unwrap_or(0)))
    });
    if ha_data.is_some() || kus_data.is_some() || slack_msgs.is_some() || ender_status.is_some() {
        controller
            .apply_fast_telemetry(ha_data, kus_data, slack_msgs, ender_status)
            .await;
    }
    if let Some(reports) = amaraki_reports {
        controller.merge_agent_reports(reports).await;
    }
    if let (Some(monitor), Some((consumption_w, solar_w))) = (power_monitor, power_sample) {
        match monitor.evaluate(consumption_w, solar_w, Instant::now()) {
            PowerAlertTransition::Trigger {
                consumption_w,
                solar_w,
            } => {
                let kw = f64::from(consumption_w) / 1000.0;
                let tts_msg = format!(
                    "Attention ! La consommation électrique de la maison est de {kw:.1} kilowatts sans production solaire. La consommation est trop importante et nous risquons une coupure de courant imminente."
                );
                let alert = ActiveAlert {
                    id: "energy-overload-7kw".to_string(),
                    severity: AlertSeverity::Critical,
                    title: "⚡ Surconsommation > 7 kW (Risque coupure)".to_string(),
                    message: format!(
                        "Consommation : {kw:.2} kW • Solaire : {solar_w} W (2 validations consécutives). Risque de coupure de courant imminente !"
                    ),
                    source: "energy-guard".to_string(),
                    dashboard_url: None,
                    screen_id: Some("org-home".to_string()),
                    triggered_at_epoch_ms: Utc::now().timestamp_millis(),
                    acknowledged: false,
                };
                controller.trigger_alert(alert).await;
                let (alexa_res, ha_res) = tokio::join!(
                    clients.alexa.announce(&tts_msg),
                    clients.ha.notify_power_overload(&tts_msg)
                );
                if let Err(err) = alexa_res {
                    warn!(%err, "Alexa MCP announce failed for power overload alert");
                }
                if let Err(err) = ha_res {
                    warn!(%err, "Home Assistant notification failed for power overload alert");
                }
            }
            PowerAlertTransition::Clear => {
                if controller
                    .snapshot()
                    .await
                    .active_alert
                    .as_ref()
                    .is_some_and(|a| a.source == "energy-guard")
                {
                    controller.clear_alert().await;
                }
            }
            PowerAlertTransition::None => {}
        }
    }
}

async fn refresh_slow_once(
    controller: &IrisController,
    weather: &OpenMeteoClient,
    ical: &IcalClient,
    k8s: &K8sClient,
    github: &GitHubClient,
    alexa: &AlexaMcpClient,
    ecoledirecte: &EcoleDirecteClient,
) {
    let org_cfg = controller.snapshot().await.settings.org;

    let weather_fut = async {
        match weather.fetch_3_days(&org_cfg).await {
            Ok(forecast) => Some(forecast),
            Err(err) => {
                warn!(%err, city = %org_cfg.weather_city, "Failed to fetch 3-day weather");
                None
            }
        }
    };
    let ical_fut = async {
        if org_cfg.ical_url.trim().is_empty() {
            return None;
        }
        match ical.fetch_week_events(&org_cfg.ical_url).await {
            Ok(events) => Some(events),
            Err(err) => {
                warn!(%err, "Failed to fetch iCal weekly planning");
                None
            }
        }
    };
    let k8s_fut = async {
        match k8s.fetch_cluster_status().await {
            Ok(status) => Some(status),
            Err(err) => {
                debug!(%err, "Kubernetes cluster status unavailable");
                None
            }
        }
    };
    let prs_fut = async {
        if org_cfg.github_repos.is_empty() {
            None
        } else {
            Some(github.fetch_open_prs(&org_cfg.github_repos).await)
        }
    };
    let alexa_fut = async {
        match alexa.fetch_shopping_list().await {
            Ok(items) => Some(items),
            Err(err) => {
                debug!(%err, "Alexa MCP shopping list unavailable");
                None
            }
        }
    };
    let ed_fut = async {
        let (ed_events, ed_status) = ecoledirecte.fetch_week_timetable().await;
        if let Some(err) = &ed_status.last_error {
            debug!(%err, configured = ed_status.configured, "ÉcoleDirecte sync status");
        }
        Some((ed_events, ed_status))
    };

    let (w, ic, k, prs, shop, ed) =
        tokio::join!(weather_fut, ical_fut, k8s_fut, prs_fut, alexa_fut, ed_fut);

    controller.apply_slow_feeds(w, ic, k, prs, shop, ed).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_overload_requires_two_consecutive_validations_and_one_minute() {
        let mut monitor = PowerOverloadMonitor::new();
        let t0 = Instant::now();

        // 1st validation > 7000 W without solar does NOT trigger immediately
        assert_eq!(monitor.evaluate(7500, 0, t0), PowerAlertTransition::None);

        // 2nd validation too early (< 55s) does NOT trigger yet
        assert_eq!(
            monitor.evaluate(7600, 0, t0 + Duration::from_secs(30)),
            PowerAlertTransition::None
        );

        // Consecutive validation at >= 60s triggers alert
        assert_eq!(
            monitor.evaluate(7800, 0, t0 + Duration::from_secs(60)),
            PowerAlertTransition::Trigger {
                consumption_w: 7800,
                solar_w: 0
            }
        );

        // Cooldown suppresses repeated notifications during ongoing overload
        assert_eq!(
            monitor.evaluate(7900, 0, t0 + Duration::from_secs(120)),
            PowerAlertTransition::None
        );

        // Solar production or drop below 7 kW clears the alert
        assert_eq!(
            monitor.evaluate(7900, 300, t0 + Duration::from_secs(180)),
            PowerAlertTransition::Clear
        );
        assert_eq!(
            monitor.evaluate(6500, 0, t0 + Duration::from_secs(240)),
            PowerAlertTransition::None
        );
    }
}
