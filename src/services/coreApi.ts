import { invoke } from "@tauri-apps/api/core";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";
import type { AppPaths, NodeConfigSnapshot } from "../types/configuration";
import type { CoreManifest, CoreVerification, SupportPackageResult } from "../types/diagnostics";
import type {
  ExplorerAddressResult,
  ExplorerLookupMode,
  ExplorerResult,
  ExplorerTransactionResult,
} from "../types/explorer";
import type {
  WalletActivityResponse,
  WalletCreateRequest,
  WalletLifecycleStatus,
  WalletLockResult,
  WalletReceiptRefreshResponse,
  WalletRecoverySelection,
  WalletRestoreRequest,
  WalletSubmissionOutcome,
  WalletTransferPreview,
} from "../types/wallet";

export function getMockDashboardSnapshot() {
  return invoke<DashboardSnapshot>("get_mock_dashboard_snapshot");
}

export function getDashboardSnapshot() {
  return invoke<DashboardSnapshot>("get_dashboard_snapshot");
}

export function getCoreProcessState() {
  return invoke<ProcessState>("get_core_process_state");
}

export function verifyCoreBinary() {
  return invoke<CoreVerification>("verify_core_binary");
}

export function getCoreManifest() {
  return invoke<CoreManifest>("get_core_manifest");
}

export function getCoreStdoutTail() {
  return invoke<string>("get_core_stdout_tail");
}

export function getCoreStderrTail() {
  return invoke<string>("get_core_stderr_tail");
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
  return invoke<SupportPackageResult>("generate_support_package");
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

export function getNodeConfigSnapshot() {
  return invoke<NodeConfigSnapshot>("get_node_config_snapshot");
}

export function getDefaultPaths() {
  return invoke<AppPaths>("get_default_paths");
}

export function walletGetStatus() {
  return invoke<WalletLifecycleStatus>("wallet_get_status");
}

export function walletSelectRecoveryDestination() {
  return invoke<WalletRecoverySelection>("wallet_select_recovery_destination");
}

export function walletCreate(request: WalletCreateRequest) {
  return invoke<WalletLifecycleStatus>("wallet_create", { request });
}

export function walletSelectRecoverySource() {
  return invoke<WalletRecoverySelection>("wallet_select_recovery_source");
}

export function walletRestore(request: WalletRestoreRequest) {
  return invoke<WalletLifecycleStatus>("wallet_restore", { request });
}

export function walletUnlock() {
  return invoke<WalletLifecycleStatus>("wallet_unlock");
}

export function walletLock() {
  return invoke<WalletLockResult>("wallet_lock");
}

export function walletPrepareTransferPreview(recipient: string, amount: string) {
  return invoke<WalletTransferPreview>("wallet_prepare_transfer_preview", {
    request: { recipient, amount },
  });
}

export function walletCancelTransferPreview(previewHandle: string) {
  return invoke<{ state: "cancelled" }>("wallet_cancel_transfer_preview", {
    request: { preview_handle: previewHandle },
  });
}

export function walletConfirmAndSubmitTransfer(previewHandle: string) {
  return invoke<WalletSubmissionOutcome>("wallet_confirm_and_submit_transfer", {
    request: { preview_handle: previewHandle },
  });
}

export function walletListActivity() {
  return invoke<WalletActivityResponse>("wallet_list_activity");
}

export function walletRefreshTransactionObservation(transactionId: string) {
  return invoke<WalletReceiptRefreshResponse>("wallet_refresh_transaction_observation", {
    request: { transaction_id: transactionId },
  });
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
