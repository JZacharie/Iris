use std::sync::Arc;
use std::time::Duration;

use iris_application::IrisController;
use iris_infrastructure::{
    AlexaMcpClient, AmarakiClient, EcoleDirecteClient, GitHubClient, HomeAssistantClient,
    IcalClient, K8sClient, KusanagiClient, OpenMeteoClient, SlackClient,
};
use tracing::{debug, warn};

pub fn spawn_org_sync_loop(controller: Arc<IrisController>) {
    let fast_ctrl = Arc::clone(&controller);
    tokio::spawn(async move {
        let ha = HomeAssistantClient::from_env();
        let kusanagi = KusanagiClient::from_env();
        let slack = SlackClient::from_env();
        let amaraki = AmarakiClient::from_env();
        loop {
            refresh_fast_once(&fast_ctrl, &ha, &kusanagi, &slack, &amaraki).await;
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
    let weather = OpenMeteoClient::new();
    let ical = IcalClient::new();
    let k8s = K8sClient::new();
    let github = GitHubClient::new();
    let alexa = AlexaMcpClient::new();
    let ecoledirecte = EcoleDirecteClient::from_env();

    tokio::join!(
        refresh_fast_once(&controller, &ha, &kusanagi, &slack, &amaraki),
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
    ha: &HomeAssistantClient,
    kusanagi: &KusanagiClient,
    slack: &SlackClient,
    amaraki: &AmarakiClient,
) {
    let ha_fut = async {
        if !ha.is_configured() {
            return None;
        }
        match ha.fetch_data().await {
            Ok(data) => Some(data),
            Err(err) => {
                warn!(%err, "Failed to fetch Home Assistant states");
                None
            }
        }
    };
    let kus_fut = async {
        match kusanagi.fetch_infra().await {
            Ok(data) => Some(data),
            Err(err) => {
                debug!(%err, "Kusanagi infra endpoints unavailable");
                None
            }
        }
    };
    let slack_fut = async {
        if !slack.is_configured() {
            return None;
        }
        match slack.fetch_recent_messages().await {
            Ok(msgs) => Some(msgs),
            Err(err) => {
                debug!(%err, "Slack recent messages unavailable");
                None
            }
        }
    };
    let amaraki_fut = async {
        match amaraki.fetch_reports().await {
            Ok(reports) => Some(reports),
            Err(err) => {
                debug!(%err, "Amaraki agent reports unavailable");
                None
            }
        }
    };

    let (ha_data, kus_data, slack_msgs, amaraki_reports) =
        tokio::join!(ha_fut, kus_fut, slack_fut, amaraki_fut);
    if ha_data.is_some() || kus_data.is_some() || slack_msgs.is_some() {
        controller
            .apply_fast_telemetry(ha_data, kus_data, slack_msgs)
            .await;
    }
    if let Some(reports) = amaraki_reports {
        controller.merge_agent_reports(reports).await;
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
