use std::sync::Arc;
use std::time::Duration;

use iris_application::IrisController;
use iris_infrastructure::{GitHubClient, IcalClient, K8sClient, OpenMeteoClient};
use tracing::{debug, warn};

pub fn spawn_org_sync_loop(controller: Arc<IrisController>) {
    tokio::spawn(async move {
        let weather = OpenMeteoClient::new();
        let ical = IcalClient::new();
        let k8s = K8sClient::new();
        let github = GitHubClient::new();
        loop {
            refresh_once(&controller, &weather, &ical, &k8s, &github).await;
            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    });
}

pub async fn refresh_org_now(controller: Arc<IrisController>) {
    let weather = OpenMeteoClient::new();
    let ical = IcalClient::new();
    let k8s = K8sClient::new();
    let github = GitHubClient::new();
    refresh_once(&controller, &weather, &ical, &k8s, &github).await;
}

async fn refresh_once(
    controller: &IrisController,
    weather: &OpenMeteoClient,
    ical: &IcalClient,
    k8s: &K8sClient,
    github: &GitHubClient,
) {
    let org_cfg = controller.snapshot().await.settings.org;

    match weather.fetch_3_days(&org_cfg).await {
        Ok(forecast) => {
            controller.set_weather(forecast).await;
        }
        Err(err) => {
            warn!(%err, city = %org_cfg.weather_city, "Failed to fetch 3-day weather");
        }
    }

    if !org_cfg.ical_url.trim().is_empty() {
        match ical.fetch_week_events(&org_cfg.ical_url).await {
            Ok(events) => {
                controller.set_ical_events(events).await;
            }
            Err(err) => {
                warn!(%err, "Failed to fetch iCal weekly planning");
            }
        }
    }

    match k8s.fetch_cluster_status().await {
        Ok(status) => {
            controller.set_cluster_status(status).await;
        }
        Err(err) => {
            debug!(%err, "Kubernetes cluster status unavailable");
        }
    }

    if !org_cfg.github_repos.is_empty() {
        let prs = github.fetch_open_prs(&org_cfg.github_repos).await;
        controller.set_open_prs(prs).await;
    }
}
