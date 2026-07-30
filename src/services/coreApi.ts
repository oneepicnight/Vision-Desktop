import { invoke } from "@tauri-apps/api/core";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";

export function getMockDashboardSnapshot() {
  return invoke<DashboardSnapshot>("get_mock_dashboard_snapshot");
}

export function getDashboardSnapshot() {
  return invoke<DashboardSnapshot>("get_dashboard_snapshot");
}

export function getCoreProcessState() {
  return invoke<ProcessState>("get_core_process_state");
}

export function startCore() {
  return invoke("start_core", { request: null });
}

export function stopCore() {
  return invoke("stop_core");
}

export function restartCore() {
  return invoke("restart_core", { request: null });
}

export function generateSupportPackage() {
  return invoke("generate_support_package");
}

export function openLogsDirectory() {
  return invoke("open_logs_directory");
}

export function openDataDirectory() {
  return invoke("open_data_directory");
}

export function saveNodeConfig(config: NodeConfig) {
  return invoke("save_node_config", { request: { config } });
}
