use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::{
    config::{allocate_api_port, load_or_create_default_config, validate_node_config, NodeConfig},
    core_manifest::{bundled_core_binary_path, verify_bundled_core_binary},
    paths::{default_paths, ensure_dir},
};

#[derive(Default)]
pub struct SupervisorState {
    inner: Mutex<Option<OwnedCoreProcess>>,
}

pub struct OwnedCoreProcess {
    child: Child,
    pid: u32,
    started_at_unix: u64,
    api_port: u16,
    p2p_port: u16,
    data_dir: PathBuf,
    log_dir: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoreProcessState {
    pub state: String,
    pub pid: Option<u32>,
    pub started_at_unix: Option<u64>,
    pub api_port: Option<u16>,
    pub p2p_port: Option<u16>,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
    pub unexpected_exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartCoreRequest {
    pub config: Option<NodeConfig>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn dir_size(path: &PathBuf) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn port_closed(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub fn process_resources(pid: u32) -> (Option<f32>, Option<u64>) {
    let mut system = System::new();
    let pid = Pid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system
        .process(pid)
        .map(|p| (Some(p.cpu_usage()), Some(p.memory())))
        .unwrap_or((None, None))
}

impl SupervisorState {
    pub fn start(&self, _request: StartCoreRequest) -> Result<CoreProcessState, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "supervisor lock poisoned".to_string())?;
        if let Some(existing) = guard.as_mut() {
            if existing
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_none()
            {
                return Err("Vision Core is already running for this desktop instance".to_string());
            }
            *guard = None;
        }

        let verification = verify_bundled_core_binary()?;
        if !verification.matches {
            return Err(format!(
                "Core binary hash mismatch: {}",
                verification.actual_sha256
            ));
        }

        return Err("Core launch blocked: frozen RC2 Core binds HTTP API to 0.0.0.0 via VISION_HTTP_PORT and has no loopback-only VISION_HTTP_ADDR setting. Desktop will not launch Core until Core can keep the administrative API private without changing consensus behavior.".to_string());

        #[allow(unreachable_code)]
        let mut cfg = _request.config.unwrap_or(load_or_create_default_config()?);
        if cfg.api_port == 0 {
            cfg.api_port = allocate_api_port()?;
        }
        validate_node_config(&cfg)?;
        ensure_dir(&cfg.data_dir)?;
        ensure_dir(&cfg.log_dir)?;

        let stdout_log = cfg.log_dir.join("vision-core.stdout.log");
        let stderr_log = cfg.log_dir.join("vision-core.stderr.log");
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stdout_log)
            .map_err(|e| format!("failed to open stdout log: {e}"))?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&stderr_log)
            .map_err(|e| format!("failed to open stderr log: {e}"))?;

        let binary = bundled_core_binary_path();
        let seed_peers = cfg.seed_peers.join(";");
        let advertised_port = cfg.advertised_port.unwrap_or(cfg.p2p_port);
        let child = Command::new(binary)
            .env("VISION_DATA_DIR", &cfg.data_dir)
            .env("VISION_HTTP_PORT", cfg.api_port.to_string())
            .env("VISION_P2P_PORT", cfg.p2p_port.to_string())
            .env("VISION_MINING", cfg.mining_enabled.to_string())
            .env("VISION_MINING_THREADS", "1")
            .env("VISION_MINER_ADDRESS", &cfg.miner_reward_address)
            .env("VISION_SEED_PEERS", seed_peers)
            .env(
                "VISION_P2P_ADVERTISED_HOST",
                cfg.advertised_host.clone().unwrap_or_default(),
            )
            .env("VISION_P2P_ADVERTISED_PORT", advertised_port.to_string())
            .env(
                "VISION_ALLOW_PRIVATE_PEERS",
                (!matches!(cfg.mode, crate::config::NodeMode::InternetNetwork)).to_string(),
            )
            .env("VISION_ALPHA_AIRDROP_ENABLED", "false")
            .env("RUST_LOG", "info")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|e| format!("failed to start Vision Core: {e}"))?;

        let pid = child.id();
        let owned = OwnedCoreProcess {
            child,
            pid,
            started_at_unix: now_unix(),
            api_port: cfg.api_port,
            p2p_port: cfg.p2p_port,
            data_dir: cfg.data_dir.clone(),
            log_dir: cfg.log_dir.clone(),
            stdout_log: stdout_log.clone(),
            stderr_log: stderr_log.clone(),
        };
        let state = owned.state("running".to_string(), None);
        *guard = Some(owned);
        Ok(state)
    }

    pub fn stop(&self) -> Result<CoreProcessState, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "supervisor lock poisoned".to_string())?;
        let Some(mut owned) = guard.take() else {
            return Ok(Self::stopped_state());
        };
        if owned.child.try_wait().map_err(|e| e.to_string())?.is_none() {
            owned.child.kill().map_err(|e| {
                format!(
                    "failed to stop owned Vision Core process {}: {e}",
                    owned.pid
                )
            })?;
            let _ = owned.child.wait();
        }
        for _ in 0..30 {
            if port_closed(owned.api_port) && port_closed(owned.p2p_port) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Ok(owned.state("stopped".to_string(), None))
    }

    pub fn current_state(&self) -> Result<CoreProcessState, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "supervisor lock poisoned".to_string())?;
        if let Some(owned) = guard.as_mut() {
            match owned.child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => Ok(owned.state("crashed".to_string(), status.code())),
                None => Ok(owned.state("running".to_string(), None)),
            }
        } else {
            Ok(Self::stopped_state())
        }
    }

    pub fn restart(&self, request: StartCoreRequest) -> Result<CoreProcessState, String> {
        let _ = self.stop()?;
        self.start(request)
    }

    pub fn stopped_state() -> CoreProcessState {
        let paths = default_paths();
        CoreProcessState {
            state: "stopped".to_string(),
            pid: None,
            started_at_unix: None,
            api_port: None,
            p2p_port: None,
            data_dir: paths.core_data,
            log_dir: paths.core_logs.clone(),
            stdout_log: paths.core_logs.join("vision-core.stdout.log"),
            stderr_log: paths.core_logs.join("vision-core.stderr.log"),
            unexpected_exit_code: None,
        }
    }

    pub fn log_paths(&self) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
        let state = self.current_state()?;
        Ok((
            state.data_dir,
            state.log_dir,
            state.stdout_log,
            state.stderr_log,
        ))
    }
}

impl OwnedCoreProcess {
    fn state(&self, state: String, exit_code: Option<i32>) -> CoreProcessState {
        CoreProcessState {
            state,
            pid: Some(self.pid),
            started_at_unix: Some(self.started_at_unix),
            api_port: Some(self.api_port),
            p2p_port: Some(self.p2p_port),
            data_dir: self.data_dir.clone(),
            log_dir: self.log_dir.clone(),
            stdout_log: self.stdout_log.clone(),
            stderr_log: self.stderr_log.clone(),
            unexpected_exit_code: exit_code,
        }
    }
}

pub fn tail_file(path: &PathBuf, max_bytes: usize) -> Result<String, String> {
    let bytes = fs::read(path).unwrap_or_default();
    let start = bytes.len().saturating_sub(max_bytes);
    Ok(String::from_utf8_lossy(&bytes[start..]).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_state_has_no_pid() {
        let state = SupervisorState::stopped_state();
        assert_eq!(state.state, "stopped");
        assert!(state.pid.is_none());
    }

    #[test]
    fn tail_file_limits_output() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("log.txt");
        std::fs::write(&file, "0123456789").unwrap();
        assert_eq!(tail_file(&file, 4).unwrap(), "6789");
    }
}
