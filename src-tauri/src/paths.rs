use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppPaths {
    pub desktop_config: PathBuf,
    pub node_config: PathBuf,
    pub core_data: PathBuf,
    pub core_logs: PathBuf,
    pub desktop_logs: PathBuf,
    pub reports: PathBuf,
    pub updates: PathBuf,
}

fn appdata() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("VisionDesktopAppData"))
}

fn localappdata() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("VisionDesktopLocalAppData"))
}

pub fn default_paths() -> AppPaths {
    let app = appdata().join("Vision").join("Desktop");
    let local_desktop = localappdata().join("Vision").join("Desktop");
    let local_core = localappdata()
        .join("Vision")
        .join("Core")
        .join("nodes")
        .join("default");
    AppPaths {
        desktop_config: app.join("config.json"),
        node_config: app.join("nodes").join("default.json"),
        core_data: local_core.join("data"),
        core_logs: local_core.join("logs"),
        desktop_logs: local_desktop.join("logs"),
        reports: local_desktop.join("reports"),
        updates: local_desktop.join("updates"),
    }
}

pub fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    Ok(())
}

pub fn ensure_dir(path: &PathBuf) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("failed to create {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_user_scoped_not_repo_relative() {
        let paths = default_paths();
        assert!(paths.desktop_config.to_string_lossy().contains("Vision"));
        assert!(!paths
            .core_data
            .to_string_lossy()
            .contains("Vision-Desktop\\src"));
    }
}
