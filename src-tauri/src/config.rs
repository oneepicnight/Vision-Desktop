use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::{IpAddr, SocketAddr, TcpListener},
    path::PathBuf,
};

use crate::paths::{default_paths, ensure_parent};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeMode {
    LocalTesting,
    PrivateNetwork,
    InternetNetwork,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConfig {
    pub node_name: String,
    pub mode: NodeMode,
    pub api_port: u16,
    pub p2p_port: u16,
    pub seed_peers: Vec<String>,
    pub advertised_host: Option<String>,
    pub advertised_port: Option<u16>,
    pub mining_enabled: bool,
    pub miner_reward_address: String,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeConfigSourceKind {
    Persisted,
    DesktopDefaultCreated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConfigSnapshot {
    pub config: NodeConfig,
    pub source_path: PathBuf,
    pub source_kind: NodeConfigSourceKind,
}

impl Default for NodeConfig {
    fn default() -> Self {
        let paths = default_paths();
        Self {
            node_name: "Default Node".to_string(),
            mode: NodeMode::LocalTesting,
            api_port: 0,
            p2p_port: 19090,
            seed_peers: Vec::new(),
            advertised_host: None,
            advertised_port: None,
            mining_enabled: false,
            miner_reward_address:
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            data_dir: paths.core_data,
            log_dir: paths.core_logs,
        }
    }
}

pub fn load_or_create_default_config() -> Result<NodeConfig, String> {
    Ok(load_node_config_snapshot()?.config)
}

pub fn load_node_config_snapshot() -> Result<NodeConfigSnapshot, String> {
    let paths = default_paths();
    if paths.node_config.exists() {
        let bytes =
            fs::read(&paths.node_config).map_err(|e| format!("failed to read node config: {e}"))?;
        let config =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid node config: {e}"))?;
        Ok(NodeConfigSnapshot {
            config,
            source_path: paths.node_config,
            source_kind: NodeConfigSourceKind::Persisted,
        })
    } else {
        let cfg = NodeConfig::default();
        save_node_config(&cfg)?;
        Ok(NodeConfigSnapshot {
            config: cfg,
            source_path: paths.node_config,
            source_kind: NodeConfigSourceKind::DesktopDefaultCreated,
        })
    }
}

pub fn save_node_config(cfg: &NodeConfig) -> Result<(), String> {
    validate_node_config(cfg)?;
    let paths = default_paths();
    ensure_parent(&paths.node_config)?;
    let json =
        serde_json::to_vec_pretty(cfg).map_err(|e| format!("failed to encode node config: {e}"))?;
    fs::write(paths.node_config, json).map_err(|e| format!("failed to write node config: {e}"))
}

pub fn validate_node_config(cfg: &NodeConfig) -> Result<(), String> {
    if cfg.node_name.trim().is_empty() {
        return Err("node name is required".to_string());
    }
    if cfg.p2p_port == 0 {
        return Err("P2P port must be stable and non-zero".to_string());
    }
    if cfg.api_port != 0 && !port_available(cfg.api_port) {
        return Err(format!("API port {} is already occupied", cfg.api_port));
    }
    if cfg.mining_enabled && !is_hex_64(&cfg.miner_reward_address) {
        return Err("miner reward address must be 64 lowercase hex characters".to_string());
    }
    for seed in &cfg.seed_peers {
        if seed.parse::<SocketAddr>().is_err() && !seed.contains(':') {
            return Err(format!("seed peer must include host and port: {seed}"));
        }
    }
    if matches!(cfg.mode, NodeMode::InternetNetwork)
        && cfg
            .advertised_host
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return Err("internet mode requires an advertised host or DNS name".to_string());
    }
    if let Some(host) = &cfg.advertised_host {
        if host == "0.0.0.0" || host == "127.0.0.1" && matches!(cfg.mode, NodeMode::InternetNetwork)
        {
            return Err("internet mode needs a reachable public advertised host".to_string());
        }
        let _ = host.parse::<IpAddr>().ok();
    }
    Ok(())
}

pub fn allocate_api_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to allocate loopback API port: {e}"))?;
    listener
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

pub fn port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn is_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_rejects_public_loopback_advertised_host() {
        let cfg = NodeConfig {
            mode: NodeMode::InternetNetwork,
            advertised_host: Some("127.0.0.1".to_string()),
            ..Default::default()
        };
        assert!(validate_node_config(&cfg).is_err());
    }

    #[test]
    fn config_requires_valid_miner_address_when_mining() {
        let cfg = NodeConfig {
            mining_enabled: true,
            miner_reward_address: "bad".to_string(),
            ..Default::default()
        };
        assert!(validate_node_config(&cfg).is_err());
    }
}
