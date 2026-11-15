use std::env;
use std::time::Duration;

use chrono::Utc;
use iris_core::{
    IrisError, KusanagiAlertItem, KusanagiAlertsSummary, KusanagiArgoCdSummary,
    KusanagiClusterMetrics, KusanagiInfraData, KusanagiNewsItem, KusanagiProxmoxNode,
    KusanagiProxmoxVm, Result,
};
use serde::Deserialize;

const DEFAULT_KUSANAGI_URL: &str = "http://kusanagi.kusanagi.svc.cluster.local";

#[derive(Deserialize)]
struct KusanagiEnvelope<T> {
    #[serde(default)]
    success: bool,
    data: Option<T>,
}

#[derive(Deserialize)]
struct RawClusterData {
    #[serde(default)]
    nodes: u32,
    #[serde(default)]
    nodes_ready: u32,
    #[serde(default)]
    pods: u32,
    #[serde(default)]
    pods_running: u32,
    #[serde(default)]
    services: u32,
}

#[derive(Deserialize)]
struct RawProxmoxNode {
    node: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    cpu: f64,
    #[serde(default)]
    mem: u64,
    #[serde(default)]
    maxmem: u64,
    #[serde(default)]
    disk: u64,
    #[serde(default)]
    maxdisk: u64,
    #[serde(default)]
    uptime: u64,
}

#[derive(Deserialize)]
struct RawProxmoxVm {
    #[serde(default)]
    vmid: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    node: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    cpu: f64,
    #[serde(default)]
    mem: u64,
    #[serde(default)]
    maxmem: u64,
}

#[derive(Deserialize)]
struct RawArgoCdData {
    #[serde(default)]
    total: u32,
    #[serde(default)]
    healthy: u32,
    #[serde(default)]
    synced: u32,
    #[serde(default)]
    degraded: u32,
    #[serde(default)]
    out_of_sync: u32,
    #[serde(default)]
    apps_with_issues: Vec<String>,
}

#[derive(Deserialize)]
struct RawAlert {
    #[serde(default)]
    name: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    severity: String,
    #[serde(default)]
    summary: String,
}

#[derive(Deserialize)]
struct RawAlertsData {
    #[serde(default)]
    critical: Vec<RawAlert>,
    #[serde(default)]
    warning: Vec<RawAlert>,
}

#[derive(Deserialize)]
struct RawNewsResponse {
    #[serde(default)]
    items: Vec<RawNewsItem>,
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Deserialize)]
struct RawNewsItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    tags: Vec<String>,
}

pub struct KusanagiClient {
    base_url: String,
    http: reqwest::Client,
}

impl KusanagiClient {
    pub fn from_env() -> Self {
        let base_url = env::var("IRIS_KUSANAGI_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_KUSANAGI_URL.to_string());

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    pub async fn fetch_infra(&self) -> Result<KusanagiInfraData> {
        let (cluster_res, proxmox_res, vms_res, lxc_res, argo_res, alerts_res, news_res) = tokio::join!(
            fetch_text(&self.http, format!("{}/api/k8s/cluster", self.base_url)),
            fetch_text(&self.http, format!("{}/api/proxmox/nodes", self.base_url)),
            fetch_text(&self.http, format!("{}/api/proxmox/vms", self.base_url)),
            fetch_text(
                &self.http,
                format!("{}/api/proxmox/containers", self.base_url)
            ),
            fetch_text(&self.http, format!("{}/api/argocd/status", self.base_url)),
            fetch_text(&self.http, format!("{}/api/alerts", self.base_url)),
            fetch_text(&self.http, format!("{}/api/news", self.base_url)),
        );

        let cluster = cluster_res.as_deref().ok().and_then(parse_cluster_json);
        let proxmox_nodes = proxmox_res
            .as_deref()
            .ok()
            .map(parse_proxmox_json)
            .unwrap_or_default();
        let mut proxmox_vms = vms_res
            .as_deref()
            .ok()
            .map(|b| parse_proxmox_vms_json(b, "qemu"))
            .unwrap_or_default();
        if let Ok(lxc_body) = lxc_res.as_deref() {
            proxmox_vms.extend(parse_proxmox_vms_json(lxc_body, "lxc"));
        }
        proxmox_vms.sort_by_key(|a| a.vmid);

        let argocd = argo_res.as_deref().ok().and_then(parse_argocd_json);
        let alerts = alerts_res.as_deref().ok().and_then(parse_alerts_json);
        let (news, news_sources) = news_res
            .as_deref()
            .ok()
            .map(parse_news_json)
            .unwrap_or_default();

        let connected = cluster.is_some()
            || !proxmox_nodes.is_empty()
            || !proxmox_vms.is_empty()
            || argocd.is_some()
            || alerts.is_some()
            || !news.is_empty();

        if !connected {
            return Err(IrisError::Persistence(
                "Kusanagi endpoints unreachable".to_string(),
            ));
        }

        Ok(KusanagiInfraData {
            connected,
            updated_at_epoch_ms: Utc::now().timestamp_millis(),
            cluster,
            proxmox_nodes,
            proxmox_vms,
            argocd,
            alerts,
            news,
            news_sources,
        })
    }
}

impl Default for KusanagiClient {
    fn default() -> Self {
        Self::from_env()
    }
}

async fn fetch_text(http: &reqwest::Client, url: String) -> Result<String> {
    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|e| IrisError::Persistence(format!("GET {url} failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(IrisError::Persistence(format!(
            "GET {url} returned {}",
            resp.status()
        )));
    }

    resp.text()
        .await
        .map_err(|e| IrisError::Persistence(format!("GET {url} body error: {e}")))
}

pub fn parse_cluster_json(body: &str) -> Option<KusanagiClusterMetrics> {
    let env: KusanagiEnvelope<RawClusterData> = serde_json::from_str(body).ok()?;
    if !env.success {
        return None;
    }
    let d = env.data?;
    Some(KusanagiClusterMetrics {
        nodes_ready: d.nodes_ready,
        nodes_total: d.nodes,
        pods_running: d.pods_running,
        pods_total: d.pods,
        services_total: d.services,
    })
}

pub fn parse_proxmox_json(body: &str) -> Vec<KusanagiProxmoxNode> {
    let Ok(env) = serde_json::from_str::<KusanagiEnvelope<Vec<RawProxmoxNode>>>(body) else {
        return Vec::new();
    };
    if !env.success {
        return Vec::new();
    }
    let mut nodes: Vec<KusanagiProxmoxNode> = env
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|n| {
            let cpu_pct = (n.cpu * 100.0).round().clamp(0.0, 100.0) as u32;
            let mem_pct = if n.maxmem > 0 {
                ((n.mem as f64 / n.maxmem as f64) * 100.0).round() as u32
            } else {
                0
            };
            let disk_pct = if n.maxdisk > 0 {
                ((n.disk as f64 / n.maxdisk as f64) * 100.0).round() as u32
            } else {
                0
            };
            KusanagiProxmoxNode {
                node: n.node,
                status: n.status,
                cpu_pct,
                mem_used_gb: format!("{:.1}", n.mem as f64 / 1_073_741_824.0),
                mem_total_gb: format!("{:.1}", n.maxmem as f64 / 1_073_741_824.0),
                mem_pct,
                disk_used_gb: format!("{:.1}", n.disk as f64 / 1_073_741_824.0),
                disk_total_gb: format!("{:.1}", n.maxdisk as f64 / 1_073_741_824.0),
                disk_pct,
                uptime_secs: n.uptime,
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.node.cmp(&b.node));
    nodes
}

pub fn parse_argocd_json(body: &str) -> Option<KusanagiArgoCdSummary> {
    let env: KusanagiEnvelope<RawArgoCdData> = serde_json::from_str(body).ok()?;
    if !env.success {
        return None;
    }
    let d = env.data?;
    Some(KusanagiArgoCdSummary {
        total: d.total,
        healthy: d.healthy,
        synced: d.synced,
        degraded: d.degraded,
        out_of_sync: d.out_of_sync,
        issues: d.apps_with_issues,
    })
}

pub fn parse_alerts_json(body: &str) -> Option<KusanagiAlertsSummary> {
    let env: KusanagiEnvelope<RawAlertsData> = serde_json::from_str(body).ok()?;
    if !env.success {
        return None;
    }
    let d = env.data?;
    let critical_count = d.critical.len() as u32;
    let warning_count = d.warning.len() as u32;
    let mut top_alerts: Vec<KusanagiAlertItem> = d
        .critical
        .into_iter()
        .chain(d.warning)
        .take(5)
        .map(|a| KusanagiAlertItem {
            name: a.name,
            namespace: a.namespace.unwrap_or_else(|| "cluster".to_string()),
            severity: a.severity,
            summary: a.summary,
        })
        .collect();
    top_alerts.truncate(5);

    Some(KusanagiAlertsSummary {
        critical_count,
        warning_count,
        top_alerts,
    })
}

pub fn parse_proxmox_vms_json(body: &str, kind: &str) -> Vec<KusanagiProxmoxVm> {
    let Ok(env) = serde_json::from_str::<KusanagiEnvelope<Vec<RawProxmoxVm>>>(body) else {
        return Vec::new();
    };
    if !env.success {
        return Vec::new();
    }
    env.data
        .unwrap_or_default()
        .into_iter()
        .map(|v| {
            let cpu_pct = (v.cpu * 100.0).round().clamp(0.0, 100.0) as u32;
            let mem_pct = if v.maxmem > 0 {
                ((v.mem as f64 / v.maxmem as f64) * 100.0).round() as u32
            } else {
                0
            };
            KusanagiProxmoxVm {
                vmid: v.vmid,
                name: v.name,
                node: v.node,
                kind: kind.to_string(),
                status: v.status,
                cpu_pct,
                mem_used_gb: format!("{:.1}", v.mem as f64 / 1_073_741_824.0),
                mem_total_gb: format!("{:.1}", v.maxmem as f64 / 1_073_741_824.0),
                mem_pct,
            }
        })
        .collect()
}

pub fn parse_news_json(body: &str) -> (Vec<KusanagiNewsItem>, Vec<String>) {
    let Ok(raw) = serde_json::from_str::<RawNewsResponse>(body) else {
        return (Vec::new(), Vec::new());
    };
    let items: Vec<KusanagiNewsItem> = raw
        .items
        .into_iter()
        .filter(|i| !i.title.trim().is_empty())
        .take(40)
        .map(|i| KusanagiNewsItem {
            title: i.title,
            description: i.description,
            source: i.source,
            icon: i.icon,
            published_at: i.published_at,
            url: i.url,
            tags: i.tags,
        })
        .collect();
    (items, raw.sources)
}

#[derive(Deserialize)]
struct PveTicketData {
    ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    csrf_token: String,
}

#[derive(Deserialize)]
struct PveTicketResponse {
    data: Option<PveTicketData>,
}

#[derive(Default, Deserialize)]
struct PveNodeEntry {
    node: String,
}

#[derive(Deserialize)]
struct PveListResponse<T> {
    data: Vec<T>,
}

#[derive(Default, Deserialize)]
struct PveVmEntry {
    vmid: u32,
    #[serde(default)]
    status: String,
}

pub async fn trigger_proxmox_updates() -> Result<u32> {
    let urls_raw = env::var("PROXMOX_URLS").unwrap_or_else(|_| {
        "https://192.168.0.50:8006,https://192.168.0.55:8006,https://192.168.0.56:8006,https://192.168.0.61:8006".to_string()
    });
    let user = env::var("PROXMOX_USER").unwrap_or_else(|_| "root@pam".to_string());
    let password = env::var("PROXMOX_PASSWORD").unwrap_or_default();

    if password.trim().is_empty() {
        return Err(IrisError::Persistence(
            "PROXMOX_PASSWORD non configuré".to_string(),
        ));
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default();

    let mut triggered = 0u32;

    for raw_url in urls_raw.split(',') {
        let base = raw_url.trim().trim_end_matches('/');
        if base.is_empty() {
            continue;
        }

        let ticket_url = format!("{base}/api2/json/access/ticket");
        let Ok(resp) = http
            .post(&ticket_url)
            .form(&[("username", user.as_str()), ("password", password.as_str())])
            .send()
            .await
        else {
            continue;
        };

        let Ok(ticket_env) = resp.json::<PveTicketResponse>().await else {
            continue;
        };
        let Some(auth) = ticket_env.data else {
            continue;
        };
        let cookie = format!("PVEAuthCookie={}", auth.ticket);

        let nodes_url = format!("{base}/api2/json/nodes");
        let nodes: Vec<String> = match http.get(&nodes_url).header("Cookie", &cookie).send().await {
            Ok(r) => r
                .json::<PveListResponse<PveNodeEntry>>()
                .await
                .map(|l| l.data.into_iter().map(|n| n.node).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        for node in nodes {
            // 1. Trigger node APT repository refresh
            let apt_url = format!("{base}/api2/json/nodes/{node}/apt/update");
            if http
                .post(&apt_url)
                .header("Cookie", &cookie)
                .header("CSRFPreventionToken", &auth.csrf_token)
                .send()
                .await
                .is_ok()
            {
                triggered += 1;
            }

            // 2. Trigger guest agent unattended apt upgrade on running QEMU VMs
            let qemu_url = format!("{base}/api2/json/nodes/{node}/qemu");
            if let Ok(r) = http.get(&qemu_url).header("Cookie", &cookie).send().await {
                if let Ok(vms) = r.json::<PveListResponse<PveVmEntry>>().await {
                    for vm in vms.data.into_iter().filter(|v| v.status == "running") {
                        let exec_url =
                            format!("{base}/api2/json/nodes/{node}/qemu/{}/agent/exec", vm.vmid);
                        if http
                            .post(&exec_url)
                            .header("Cookie", &cookie)
                            .header("CSRFPreventionToken", &auth.csrf_token)
                            .form(&[
                                ("command", "/bin/sh"),
                                (
                                    "input-data",
                                    "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get upgrade -y\n",
                                ),
                            ])
                            .send()
                            .await
                            .is_ok()
                        {
                            triggered += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(triggered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kusanagi_payloads() {
        let cluster = parse_cluster_json(
            r#"{"success":true,"data":{"nodes":16,"nodes_ready":16,"pods":529,"pods_running":529,"services":559}}"#,
        );
        assert_eq!(cluster.as_ref().map(|c| c.pods_running), Some(529));

        let proxmox = parse_proxmox_json(
            r#"{"success":true,"data":[{"node":"aquabot","status":"online","cpu":0.605,"mem":10737418240,"maxmem":21474836480,"disk":1073741824,"maxdisk":10737418240,"uptime":44805}]}"#,
        );
        assert_eq!(proxmox.len(), 1);
        assert_eq!(proxmox[0].cpu_pct, 61);
        assert_eq!(proxmox[0].mem_pct, 50);

        let argo = parse_argocd_json(
            r#"{"success":true,"data":{"total":211,"healthy":211,"synced":211,"degraded":0,"out_of_sync":0,"apps_with_issues":[]}}"#,
        );
        assert_eq!(argo.as_ref().map(|a| a.total), Some(211));

        let alerts = parse_alerts_json(
            r#"{"success":true,"data":{"critical":[{"name":"PVFilling","namespace":"youtubedl","severity":"critical","summary":"PV almost full"}],"warning":[]}}"#,
        );
        assert_eq!(alerts.as_ref().map(|a| a.critical_count), Some(1));

        let (news, sources) = parse_news_json(
            r#"{"cached_at":"2026-11-15T05:10:51Z","items":[{"title":"Rust 1.90","description":"Nouveautés","source":"hackernews","icon":"🦀","published_at":"2026-11-15T04:00:00Z","url":"https://example.com","tags":["rust"]}],"sources":["hackernews","aws"]}"#,
        );
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].source, "hackernews");
        assert_eq!(sources.len(), 2);
    }
}
