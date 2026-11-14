use std::time::Duration;

use chrono::Utc;
use iris_core::{ClusterStatus, IrisError, Result};
use serde::Deserialize;

const SA_TOKEN_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";
const SA_CA_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";
const K8S_NODES_URL: &str = "https://kubernetes.default.svc/api/v1/nodes";

#[derive(Deserialize)]
struct K8sNodeList {
    #[serde(default)]
    items: Vec<K8sNode>,
}

#[derive(Deserialize)]
struct K8sNode {
    metadata: K8sMetadata,
    status: Option<K8sNodeStatus>,
}

#[derive(Deserialize)]
struct K8sMetadata {
    name: String,
}

#[derive(Deserialize)]
struct K8sNodeStatus {
    #[serde(default)]
    conditions: Vec<K8sNodeCondition>,
}

#[derive(Deserialize)]
struct K8sNodeCondition {
    #[serde(rename = "type")]
    condition_type: String,
    status: String,
}

pub struct K8sClient;

impl K8sClient {
    pub fn new() -> Self {
        Self
    }

    pub async fn fetch_cluster_status(&self) -> Result<ClusterStatus> {
        let token = tokio::fs::read_to_string(SA_TOKEN_PATH)
            .await
            .map_err(|e| {
                IrisError::InvalidConfig(format!("k8s serviceaccount token unavailable: {e}"))
            })?;
        let token = token.trim().to_string();

        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(10));
        if let Ok(ca_bytes) = tokio::fs::read(SA_CA_PATH).await {
            if let Ok(cert) = reqwest::Certificate::from_pem(&ca_bytes) {
                builder = builder.add_root_certificate(cert);
            }
        }

        let http = builder
            .build()
            .map_err(|e| IrisError::Persistence(format!("k8s http client error: {e}")))?;

        let resp = http
            .get(K8S_NODES_URL)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| IrisError::Persistence(format!("k8s nodes request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(IrisError::Persistence(format!(
                "k8s nodes API returned status {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| IrisError::Persistence(format!("k8s nodes body error: {e}")))?;

        parse_nodes_json(&body, Utc::now().timestamp_millis())
    }
}

impl Default for K8sClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_nodes_json(body: &str, checked_at_epoch_ms: i64) -> Result<ClusterStatus> {
    let list: K8sNodeList = serde_json::from_str(body)
        .map_err(|e| IrisError::Persistence(format!("invalid k8s nodes JSON: {e}")))?;

    let mut nodes_total: u32 = 0;
    let mut nodes_ready: u32 = 0;
    let mut offline_nodes = Vec::new();

    for node in list.items {
        nodes_total = nodes_total.saturating_add(1);
        let is_ready = node
            .status
            .as_ref()
            .map(|st| {
                st.conditions
                    .iter()
                    .any(|c| c.condition_type == "Ready" && c.status == "True")
            })
            .unwrap_or(false);

        if is_ready {
            nodes_ready = nodes_ready.saturating_add(1);
        } else {
            offline_nodes.push(node.metadata.name);
        }
    }

    Ok(ClusterStatus {
        nodes_total,
        nodes_ready,
        offline_nodes,
        checked_at_epoch_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_nodes_json;

    #[test]
    fn detects_ready_and_offline_nodes() {
        let json = r#"{
            "items": [
                {
                    "metadata": { "name": "jo3-cp-1" },
                    "status": {
                        "conditions": [
                            { "type": "MemoryPressure", "status": "False" },
                            { "type": "Ready", "status": "True" }
                        ]
                    }
                },
                {
                    "metadata": { "name": "jo3-worker-2" },
                    "status": {
                        "conditions": [
                            { "type": "Ready", "status": "Unknown" }
                        ]
                    }
                }
            ]
        }"#;

        let status = parse_nodes_json(json, 12345).unwrap();
        assert_eq!(status.nodes_total, 2);
        assert_eq!(status.nodes_ready, 1);
        assert_eq!(status.offline_nodes, vec!["jo3-worker-2".to_string()]);
        assert_eq!(status.checked_at_epoch_ms, 12345);
    }
}
