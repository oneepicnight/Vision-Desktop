import { invoke } from "@tauri-apps/api/core";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";
import type {
  ExplorerAddressResult,
  ExplorerLookupMode,
  ExplorerResult,
  ExplorerTransactionResult,
} from "../types/explorer";

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

export function lookupExplorerAddress(query: string) {
  return invoke<ExplorerAddressResult>("lookup_explorer_address", {
    request: { query },
  });
}

export function lookupExplorerTransaction(query: string) {
  return invoke<ExplorerTransactionResult>("lookup_explorer_transaction", {
    request: { query },
  });
}

export async function searchMockExplorer(
  mode: ExplorerLookupMode,
  query: string,
): Promise<ExplorerResult> {
  const value = query.trim();
  if (mode === "address") {
    return {
      kind: "address",
      address: value || "VISION_TEST_ADDRESS",
      balance: "300003",
      nonce: "3",
    };
  }

  return {
    kind: "transaction",
    txid: value || "demo-transaction-id",
    payload: JSON.stringify(
      {
        txid: value || "demo-transaction-id",
        status: "canonical",
        block_height: 128,
        sender: "VISION_TEST_SENDER",
        recipient: "VISION_TEST_RECIPIENT",
        amount: 100001,
      },
      null,
      2,
    ),
  };
}
