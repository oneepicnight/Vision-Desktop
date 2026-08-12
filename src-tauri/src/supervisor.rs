use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::{
    fs::{self, OpenOptions},
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, ProcessesToUpdate, System};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, FILETIME, HANDLE, STILL_ACTIVE},
    System::Threading::{GetCurrentProcess, GetExitCodeProcess, GetProcessId, GetProcessTimes},
};

use crate::{
    config::{allocate_api_port, load_or_create_default_config, validate_node_config, NodeConfig},
    core_manifest::{
        bundled_core_binary_path, load_wallet_core_compatibility, verify_bundled_core_binary,
    },
    paths::{default_paths, ensure_dir},
};

pub struct SupervisorState {
    inner: Mutex<Option<OwnedCoreProcess>>,
    next_generation: AtomicU64,
}

pub struct OwnedCoreProcess {
    child: Child,
    pid: u32,
    generation: u64,
    started_at_unix: u64,
    api_port: u16,
    p2p_port: u16,
    data_dir: PathBuf,
    log_dir: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreAuthorityError {
    UnsupportedCompatibility,
    CoreUnavailable,
    CoreIdentityChanged,
}

#[cfg(windows)]
pub(crate) struct CoreConnectionAuthority<'a> {
    supervisor: &'a SupervisorState,
    held_process: OwnedHandle,
    pid: u32,
    process_created_at: u64,
    generation: u64,
    api_port: u16,
    compatibility_fingerprint: [u8; 32],
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

impl Default for SupervisorState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            next_generation: AtomicU64::new(0),
        }
    }
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
    fn issue_generation(&self) -> Result<u64, String> {
        self.next_generation
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| "Core process generation exhausted".to_string())
    }

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
        let generation = self.issue_generation()?;
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
            generation,
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

    #[cfg(windows)]
    pub(crate) fn wallet_core_connection_authority(
        &self,
    ) -> Result<CoreConnectionAuthority<'_>, CoreAuthorityError> {
        let compatibility = load_wallet_core_compatibility()
            .map_err(|_| CoreAuthorityError::UnsupportedCompatibility)?;
        let verification = verify_bundled_core_binary()
            .map_err(|_| CoreAuthorityError::UnsupportedCompatibility)?;
        if !verification.matches {
            return Err(CoreAuthorityError::UnsupportedCompatibility);
        }

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| CoreAuthorityError::CoreUnavailable)?;
        let owned = guard.as_mut().ok_or(CoreAuthorityError::CoreUnavailable)?;
        if owned
            .child
            .try_wait()
            .map_err(|_| CoreAuthorityError::CoreUnavailable)?
            .is_some()
        {
            return Err(CoreAuthorityError::CoreUnavailable);
        }

        let held_process = duplicate_process_handle(owned.child.as_raw_handle())?;
        let process_created_at = process_creation_identity(&held_process)?;
        if get_process_id_checked(&held_process)? != owned.pid {
            return Err(CoreAuthorityError::CoreIdentityChanged);
        }

        let authority = CoreConnectionAuthority {
            supervisor: self,
            held_process,
            pid: owned.pid,
            process_created_at,
            generation: owned.generation,
            api_port: owned.api_port,
            compatibility_fingerprint: compatibility.manifest_sha256(),
        };
        drop(guard);
        authority.validate()?;
        Ok(authority)
    }
}

#[cfg(windows)]
impl CoreConnectionAuthority<'_> {
    pub(crate) fn api_port(&self) -> u16 {
        self.api_port
    }

    pub(crate) fn expected_pid(&self) -> u32 {
        self.pid
    }

    pub(crate) fn wallet_identity_fingerprint(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vision-desktop.wallet-core-connection-identity.v1");
        hasher.update(&[0]);
        hasher.update(&self.pid.to_le_bytes());
        hasher.update(&self.process_created_at.to_le_bytes());
        hasher.update(&self.generation.to_le_bytes());
        hasher.update(&self.api_port.to_le_bytes());
        hasher.update(&[127, 0, 0, 1]);
        hasher.update(&self.compatibility_fingerprint);
        *hasher.finalize().as_bytes()
    }

    pub(crate) fn validate(&self) -> Result<(), CoreAuthorityError> {
        if !process_is_alive(&self.held_process)?
            || get_process_id_checked(&self.held_process)? != self.pid
            || process_creation_identity(&self.held_process)? != self.process_created_at
        {
            return Err(CoreAuthorityError::CoreIdentityChanged);
        }

        let compatibility = load_wallet_core_compatibility()
            .map_err(|_| CoreAuthorityError::CoreIdentityChanged)?;
        let verification =
            verify_bundled_core_binary().map_err(|_| CoreAuthorityError::CoreIdentityChanged)?;
        if !verification.matches
            || compatibility.manifest_sha256() != self.compatibility_fingerprint
        {
            return Err(CoreAuthorityError::CoreIdentityChanged);
        }

        let mut guard = self
            .supervisor
            .inner
            .lock()
            .map_err(|_| CoreAuthorityError::CoreIdentityChanged)?;
        let current = guard
            .as_mut()
            .ok_or(CoreAuthorityError::CoreIdentityChanged)?;
        if current.generation != self.generation
            || current.pid != self.pid
            || current.api_port != self.api_port
            || current
                .child
                .try_wait()
                .map_err(|_| CoreAuthorityError::CoreIdentityChanged)?
                .is_some()
            || process_creation_identity_from_raw(current.child.as_raw_handle())?
                != self.process_created_at
        {
            return Err(CoreAuthorityError::CoreIdentityChanged);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn duplicate_process_handle(raw: RawHandle) -> Result<OwnedHandle, CoreAuthorityError> {
    let current = unsafe { GetCurrentProcess() };
    let mut duplicate: HANDLE = std::ptr::null_mut();
    let succeeded = unsafe {
        DuplicateHandle(
            current,
            raw as HANDLE,
            current,
            &mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if succeeded == 0 || duplicate.is_null() {
        return Err(CoreAuthorityError::CoreUnavailable);
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(duplicate as RawHandle) })
}

#[cfg(windows)]
fn get_process_id_checked(handle: &OwnedHandle) -> Result<u32, CoreAuthorityError> {
    let pid = unsafe { GetProcessId(handle.as_raw_handle() as HANDLE) };
    if pid == 0 {
        Err(CoreAuthorityError::CoreIdentityChanged)
    } else {
        Ok(pid)
    }
}

#[cfg(windows)]
fn process_creation_identity(handle: &OwnedHandle) -> Result<u64, CoreAuthorityError> {
    process_creation_identity_from_raw(handle.as_raw_handle())
}

#[cfg(windows)]
fn process_creation_identity_from_raw(raw: RawHandle) -> Result<u64, CoreAuthorityError> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let succeeded = unsafe {
        GetProcessTimes(
            raw as HANDLE,
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if succeeded == 0 {
        return Err(CoreAuthorityError::CoreIdentityChanged);
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(windows)]
fn process_is_alive(handle: &OwnedHandle) -> Result<bool, CoreAuthorityError> {
    let mut code = 0_u32;
    let succeeded = unsafe { GetExitCodeProcess(handle.as_raw_handle() as HANDLE, &mut code) };
    if succeeded == 0 {
        return Err(CoreAuthorityError::CoreIdentityChanged);
    }
    Ok(code == STILL_ACTIVE as u32)
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
    fn process_generations_are_monotonic_and_never_reused() {
        let supervisor = SupervisorState::default();
        assert_eq!(supervisor.issue_generation().unwrap(), 1);
        assert_eq!(supervisor.issue_generation().unwrap(), 2);
        assert_eq!(supervisor.issue_generation().unwrap(), 3);
    }

    #[test]
    fn tail_file_limits_output() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("log.txt");
        std::fs::write(&file, "0123456789").unwrap();
        assert_eq!(tail_file(&file, 4).unwrap(), "6789");
    }
}
