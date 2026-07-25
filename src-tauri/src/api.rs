use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MiningStatusSnapshot {
    pub available: bool,
    pub active: bool,
    pub blocks_found: u64,
    pub recovery_state: String,
    pub paused_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryStatusSnapshot {
    pub state: String,
    pub peer_addr: Option<String>,
    pub local_height: Option<u64>,
    pub local_work: Option<u128>,
    pub local_tip_hash: Option<String>,
    pub remote_height: Option<u64>,
    pub remote_work: Option<u128>,
    pub remote_tip_hash: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeStatusSnapshot {
    pub version: String,
    pub canonical_tip_height: u64,
    pub canonical_tip_hash: String,
    pub cached_state_root_height: Option<u64>,
    pub cached_state_root: Option<String>,
    pub mempool_size: usize,
    pub peer_count: usize,
    pub durable_peer_count: usize,
    pub active_inbound_sessions: usize,
    pub active_outbound_sessions: usize,
    pub transient_peer_count: usize,
    pub dialable_peer_count: usize,
    pub mining: MiningStatusSnapshot,
    pub recovery: RecoveryStatusSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MiningInfoResponse {
    pub enabled: bool,
    pub height: u64,
    pub difficulty: u64,
    pub epoch: u64,
    pub active: bool,
    pub recovery_state: String,
    pub paused_reason: Option<String>,
    pub hash_rate_estimate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerEntry {
    pub addr: String,
    pub state: String,
    pub height: u64,
    pub outbound: bool,
    pub height_age_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardSnapshot {
    pub process_state: String,
    pub status: Option<NodeStatusSnapshot>,
    pub mining: Option<MiningInfoResponse>,
    pub peers: Vec<PeerEntry>,
    pub api_error: Option<String>,
    pub core_cpu: Option<f32>,
    pub core_memory_bytes: Option<u64>,
    pub data_dir_size_bytes: u64,
    pub log_dir_size_bytes: u64,
    pub mock_mode: bool,
}

pub fn fetch_dashboard(
    api_port: u16,
    process_state: String,
    data_dir_size_bytes: u64,
    log_dir_size_bytes: u64,
) -> DashboardSnapshot {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build();
    let Ok(client) = client else {
        return DashboardSnapshot {
            process_state,
            status: None,
            mining: None,
            peers: Vec::new(),
            api_error: Some("failed to create API client".to_string()),
            core_cpu: None,
            core_memory_bytes: None,
            data_dir_size_bytes,
            log_dir_size_bytes,
            mock_mode: false,
        };
    };
    let base = format!("http://127.0.0.1:{api_port}");
    let status = client
        .get(format!("{base}/status"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json::<NodeStatusSnapshot>());
    match status {
        Ok(status) => {
            let mining = client
                .get(format!("{base}/mining/info"))
                .send()
                .ok()
                .and_then(|r| r.error_for_status().ok())
                .and_then(|r| r.json::<MiningInfoResponse>().ok());
            let peers = client
                .get(format!("{base}/peers"))
                .send()
                .ok()
                .and_then(|r| r.error_for_status().ok())
                .and_then(|r| r.json::<Vec<PeerEntry>>().ok())
                .unwrap_or_default();
            DashboardSnapshot {
                process_state,
                status: Some(status),
                mining,
                peers,
                api_error: None,
                core_cpu: None,
                core_memory_bytes: None,
                data_dir_size_bytes,
                log_dir_size_bytes,
                mock_mode: false,
            }
        }
        Err(e) => DashboardSnapshot {
            process_state,
            status: None,
            mining: None,
            peers: Vec::new(),
            api_error: Some(e.to_string()),
            core_cpu: None,
            core_memory_bytes: None,
            data_dir_size_bytes,
            log_dir_size_bytes,
            mock_mode: false,
        },
    }
}

pub fn mock_dashboard() -> DashboardSnapshot {
    DashboardSnapshot {
        process_state: "running".to_string(),
        status: Some(NodeStatusSnapshot {
            version: "3".to_string(),
            canonical_tip_height: 128,
            canonical_tip_hash: "0062241f...2a032".to_string(),
            cached_state_root_height: Some(128),
            cached_state_root: Some("c7eaacfc...ffbd".to_string()),
            mempool_size: 0,
            peer_count: 3,
            durable_peer_count: 3,
            active_inbound_sessions: 1,
            active_outbound_sessions: 2,
            transient_peer_count: 0,
            dialable_peer_count: 3,
            mining: MiningStatusSnapshot {
                available: true,
                active: true,
                blocks_found: 42,
                recovery_state: "normal".to_string(),
                paused_reason: None,
            },
            recovery: RecoveryStatusSnapshot {
                state: "normal".to_string(),
                peer_addr: None,
                local_height: None,
                local_work: Some(14624),
                local_tip_hash: None,
                remote_height: None,
                remote_work: None,
                remote_tip_hash: None,
                reason: None,
            },
        }),
        mining: Some(MiningInfoResponse {
            enabled: true,
            height: 128,
            difficulty: 3,
            epoch: 0,
            active: true,
            recovery_state: "normal".to_string(),
            paused_reason: None,
            hash_rate_estimate: None,
        }),
        peers: vec![PeerEntry {
            addr: "seed.example:37072".to_string(),
            state: "connected".to_string(),
            height: 128,
            outbound: true,
            height_age_secs: Some(1),
        }],
        api_error: None,
        core_cpu: Some(3.2),
        core_memory_bytes: Some(64 * 1024 * 1024),
        data_dir_size_bytes: 42_000_000,
        log_dir_size_bytes: 250_000,
        mock_mode: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_json() {
        let json = r#"{"version":"3","canonical_tip_height":1,"canonical_tip_hash":"abc","cached_state_root_height":1,"cached_state_root":"def","mempool_size":0,"peer_count":0,"durable_peer_count":0,"active_inbound_sessions":0,"active_outbound_sessions":0,"transient_peer_count":0,"dialable_peer_count":0,"mining":{"available":false,"active":false,"blocks_found":0,"recovery_state":"normal","paused_reason":null},"recovery":{"state":"normal","peer_addr":null,"local_height":null,"local_work":null,"local_tip_hash":null,"remote_height":null,"remote_work":null,"remote_tip_hash":null,"reason":null}}"#;
        let parsed: NodeStatusSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.canonical_tip_height, 1);
        assert_eq!(parsed.recovery.state, "normal");
    }
}
