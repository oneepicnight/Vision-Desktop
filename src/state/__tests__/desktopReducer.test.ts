import { applyDesktopEvent } from "../desktopReducer";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import type { ConfigurationState } from "../../types/configuration";
import type { DiagnosticsState } from "../../types/diagnostics";
import type { ExplorerAddressResult, ExplorerTransactionResult } from "../../types/explorer";
import type { WalletAccountState } from "../../types/wallet";
import type { DesktopState } from "../desktopState";

function assertEqual<T>(actual: T, expected: T, message?: string) {
  if (actual !== expected) {
    throw new Error(message ?? `Expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertDeepEqual(actual: unknown, expected: unknown, message?: string) {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) {
    throw new Error(message ?? `Expected ${expectedJson}, got ${actualJson}`);
  }
}
const baseConfig: NodeConfig = {
  node_name: "Default Node",
  mode: "LocalTesting",
  api_port: 0,
  p2p_port: 19090,
  seed_peers: [],
  advertised_host: null,
  advertised_port: null,
  mining_enabled: false,
  miner_reward_address: "0000000000000000000000000000000000000000000000000000000000000000",
  data_dir: "",
  log_dir: "",
};

const alternateConfig: NodeConfig = {
  ...baseConfig,
  node_name: "Peer Node",
  mode: "PrivateNetwork",
  p2p_port: 19091,
  seed_peers: ["seed.example:19090"],
};

const baseSnapshot: DashboardSnapshot = {
  process_state: "running",
  status: {
    version: "3",
    canonical_tip_height: 10,
    canonical_tip_hash: "tip-a",
    cached_state_root_height: 10,
    cached_state_root: "root-a",
    mempool_size: 1,
    peer_count: 2,
    durable_peer_count: 1,
    active_inbound_sessions: 1,
    active_outbound_sessions: 1,
    transient_peer_count: 0,
    dialable_peer_count: 2,
    mining: {
      available: true,
      active: false,
      blocks_found: 3,
      recovery_state: "normal",
      paused_reason: null,
    },
    recovery: {
      state: "normal",
      peer_addr: null,
      local_height: 10,
      local_work: 100,
      local_tip_hash: "tip-a",
      remote_height: null,
      remote_work: null,
      remote_tip_hash: null,
      reason: null,
    },
  },
  mining: {
    enabled: true,
    height: 10,
    difficulty: 3,
    epoch: 0,
    active: false,
    recovery_state: "normal",
    paused_reason: null,
    hash_rate_estimate: null,
  },
  peers: [{ addr: "seed.example:19090", state: "connected", height: 10, outbound: true, height_age_secs: 1 }],
  api_error: null,
  core_cpu: 1.5,
  core_memory_bytes: 1024,
  data_dir_size_bytes: 2048,
  log_dir_size_bytes: 4096,
  mock_mode: false,
};

const baseProcess: ProcessState = {
  state: "running",
  pid: 1234,
  api_port: 18080,
  p2p_port: 19090,
  data_dir: "data",
  log_dir: "logs",
};

const baseDiagnostics: DiagnosticsState = {
  manifest: null,
  verification: null,
  stdoutTail: null,
  stderrTail: null,
  error: null,
};

const baseWallet: WalletAccountState = {
  queriedAddress: null,
  account: null,
  error: null,
};

const baseConfiguration: ConfigurationState = {
  snapshot: null,
  appPaths: null,
  error: null,
};

const baseState: DesktopState = {
  activeView: "dashboard",
  mockMode: true,
  snapshot: null,
  process: null,
  message: "Ready",
  loading: false,
  error: "previous error",
  wizardOpen: false,
  config: baseConfig,
  explorer: {
    mode: "address",
    query: "",
    result: null,
    loading: false,
    error: null,
  },
  diagnostics: baseDiagnostics,
  configuration: baseConfiguration,
  wallet: baseWallet,
  lastUpdatedAt: null,
  activeLifecycleAction: null,
  pendingLifecycleConfirmation: null,
};

const addressResult: ExplorerAddressResult = {
  kind: "address",
  address: "addr-1",
  balance: "100",
  nonce: "2",
};

const transactionResult: ExplorerTransactionResult = {
  kind: "transaction",
  txid: "tx-1",
  payload: "{\n  \"txid\": \"tx-1\"\n}",
};

function expectPreserved(previous: DesktopState, next: DesktopState, changedKeys: Array<keyof DesktopState>) {
  const changed = new Set<keyof DesktopState>(changedKeys);
  for (const key of Object.keys(previous) as Array<keyof DesktopState>) {
    if (!changed.has(key)) {
      assertDeepEqual(next[key], previous[key], `${key} should be preserved`);
    }
  }
}

export function runDesktopReducerTransitionTests() {
  {
    const next = applyDesktopEvent(baseState, { type: "ActiveViewChanged", view: "explorer" });
    assertEqual(next.activeView, "explorer");
    expectPreserved(baseState, next, ["activeView"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "ActiveViewChanged", view: "wallet" });
    assertEqual(next.activeView, "wallet");
    expectPreserved(baseState, next, ["activeView"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "ActiveViewChanged", view: "peers" });
    assertEqual(next.activeView, "peers");
    expectPreserved(baseState, next, ["activeView"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "ActiveViewChanged", view: "mining" });
    assertEqual(next.activeView, "mining");
    expectPreserved(baseState, next, ["activeView"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "ActiveViewChanged", view: "diagnostics" });
    assertEqual(next.activeView, "diagnostics");
    expectPreserved(baseState, next, ["activeView"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "ActiveViewChanged", view: "configuration" });
    assertEqual(next.activeView, "configuration");
    expectPreserved(baseState, next, ["activeView"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "MockModeChanged", mockMode: false });
    assertEqual(next.mockMode, false);
    expectPreserved(baseState, next, ["mockMode"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "WizardOpenChanged", open: true });
    assertEqual(next.wizardOpen, true);
    expectPreserved(baseState, next, ["wizardOpen"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "NodeConfigChanged", config: alternateConfig });
    assertDeepEqual(next.config, alternateConfig);
    expectPreserved(baseState, next, ["config"]);
  }

  {
    const stateWithExplorer = {
      ...baseState,
      explorer: { ...baseState.explorer, query: "addr-1", result: addressResult },
    };
    const next = applyDesktopEvent(stateWithExplorer, { type: "ExplorerModeChanged", mode: "transaction" });
    assertEqual(next.explorer.mode, "transaction");
    assertEqual(next.explorer.query, "");
    assertEqual(next.explorer.result, null);
    expectPreserved(stateWithExplorer, next, ["explorer"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "ExplorerQueryChanged", query: "addr-1" });
    assertEqual(next.explorer.query, "addr-1");
    expectPreserved(baseState, next, ["explorer"]);
  }

  {
    const next = applyDesktopEvent(baseState, {
      type: "ExplorerLookupStarted",
      message: "Looking up address...",
    });
    assertEqual(next.explorer.loading, true);
    assertEqual(next.explorer.error, null);
    assertEqual(next.message, "Looking up address...");
    expectPreserved(baseState, next, ["explorer", "message"]);
  }

  {
    const loadingState = applyDesktopEvent(baseState, {
      type: "ExplorerLookupStarted",
      message: "Looking up address...",
    });
    const next = applyDesktopEvent(loadingState, {
      type: "ExplorerResultUpdated",
      result: addressResult,
      message: "Address lookup complete",
    });
    assertDeepEqual(next.explorer.result, addressResult);
    assertEqual(next.explorer.loading, false);
    assertEqual(next.explorer.error, null);
    assertEqual(next.message, "Address lookup complete");
  }

  {
    const loadingState = applyDesktopEvent(baseState, {
      type: "ExplorerLookupStarted",
      message: "Looking up transaction...",
    });
    const next = applyDesktopEvent(loadingState, {
      type: "ExplorerResultUpdated",
      result: transactionResult,
      message: "Transaction lookup complete",
    });
    assertDeepEqual(next.explorer.result, transactionResult);
    assertEqual(next.explorer.loading, false);
    assertEqual(next.message, "Transaction lookup complete");
  }

  {
    const loadingState = applyDesktopEvent(baseState, {
      type: "ExplorerLookupStarted",
      message: "Looking up address...",
    });
    const next = applyDesktopEvent(loadingState, {
      type: "ExplorerLookupFailed",
      message: "lookup failed",
    });
    assertEqual(next.explorer.loading, false);
    assertEqual(next.explorer.error, "lookup failed");
    assertEqual(next.message, "lookup failed");
  }

  {
    const stateWithExplorer = {
      ...baseState,
      explorer: { ...baseState.explorer, query: "addr-1", result: addressResult, error: "old" },
    };
    const next = applyDesktopEvent(stateWithExplorer, { type: "ExplorerLookupCleared" });
    assertEqual(next.explorer.result, null);
    assertEqual(next.explorer.error, null);
    assertEqual(next.explorer.loading, false);
    assertEqual(next.explorer.query, "addr-1");
  }

  {
    const next = applyDesktopEvent(baseState, { type: "DashboardRefreshStarted" });
    assertEqual(next.loading, true);
    assertEqual(next.error, null);
    expectPreserved(baseState, next, ["loading", "error"]);
  }

  {
    const next = applyDesktopEvent(baseState, {
      type: "DashboardSnapshotUpdated",
      snapshot: baseSnapshot,
      message: "Dashboard refreshed",
      receivedAt: 1234,
    });
    assertDeepEqual(next.snapshot, baseSnapshot);
    assertEqual(next.message, "Dashboard refreshed");
    assertEqual(next.lastUpdatedAt, 1234);
    expectPreserved(baseState, next, ["snapshot", "message", "lastUpdatedAt"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "CoreProcessUpdated", process: baseProcess });
    assertDeepEqual(next.process, baseProcess);
    expectPreserved(baseState, next, ["process"]);
  }

  {
    const next = applyDesktopEvent(baseState, {
      type: "LifecycleConfirmationRequested",
      action: "restart",
    });
    assertEqual(next.pendingLifecycleConfirmation, "restart");
    expectPreserved(baseState, next, ["pendingLifecycleConfirmation"]);
  }

  {
    const confirmState = applyDesktopEvent(baseState, {
      type: "LifecycleConfirmationRequested",
      action: "restart",
    });
    const next = applyDesktopEvent(confirmState, { type: "LifecycleConfirmationDismissed" });
    assertEqual(next.pendingLifecycleConfirmation, null);
    expectPreserved(confirmState, next, ["pendingLifecycleConfirmation"]);
  }

  {
    const next = applyDesktopEvent(baseState, {
      type: "LifecycleActionStarted",
      action: "start",
      message: "Start command requested...",
    });
    assertEqual(next.activeLifecycleAction, "start");
    assertEqual(next.loading, true);
    assertEqual(next.error, null);
    assertEqual(next.message, "Start command requested...");
    expectPreserved(baseState, next, ["activeLifecycleAction", "loading", "error", "message", "pendingLifecycleConfirmation"]);
  }

  {
    const started = applyDesktopEvent(baseState, {
      type: "LifecycleActionStarted",
      action: "restart",
      message: "Restart command requested...",
    });
    const next = applyDesktopEvent(started, {
      type: "LifecycleActionCompleted",
      action: "restart",
      message: "Restart command completed; confirm the observed process state below.",
    });
    assertEqual(next.activeLifecycleAction, null);
    assertEqual(next.loading, false);
    assertEqual(next.message, "Restart command completed; confirm the observed process state below.");
  }

  {
    const started = applyDesktopEvent(baseState, {
      type: "LifecycleActionStarted",
      action: "stop",
      message: "Stop command requested...",
    });
    const next = applyDesktopEvent(started, {
      type: "LifecycleActionFailed",
      action: "stop",
      message: "stop failed",
    });
    assertEqual(next.activeLifecycleAction, null);
    assertEqual(next.loading, false);
    assertEqual(next.error, "stop failed");
    assertEqual(next.message, "stop failed");
  }

  {
    const diagnostics: DiagnosticsState = {
      manifest: null,
      verification: {
        binary_path: "C:\\Vision\\vision-core.exe",
        expected_sha256: "abc",
        actual_sha256: "abc",
        matches: true,
      },
      stdoutTail: "stdout",
      stderrTail: "stderr",
      error: null,
    };
    const next = applyDesktopEvent(baseState, { type: "DiagnosticsUpdated", diagnostics });
    assertDeepEqual(next.diagnostics, diagnostics);
    expectPreserved(baseState, next, ["diagnostics"]);
  }

  {
    const configuration: ConfigurationState = {
      snapshot: {
        config: baseConfig,
        source_path: "C:\\Users\\operator\\AppData\\Roaming\\Vision\\Desktop\\nodes\\default.json",
        source_kind: "persisted",
      },
      appPaths: {
        desktop_config: "C:\\Users\\operator\\AppData\\Roaming\\Vision\\Desktop\\config.json",
        node_config: "C:\\Users\\operator\\AppData\\Roaming\\Vision\\Desktop\\nodes\\default.json",
        core_data: "data",
        core_logs: "logs",
        desktop_logs: "desktop-logs",
        reports: "reports",
        updates: "updates",
      },
      error: null,
    };
    const next = applyDesktopEvent(baseState, { type: "ConfigurationUpdated", configuration });
    assertDeepEqual(next.configuration, configuration);
    expectPreserved(baseState, next, ["configuration"]);
  }

  {
    const wallet: WalletAccountState = {
      queriedAddress: "abcd",
      account: addressResult,
      error: null,
    };
    const next = applyDesktopEvent(baseState, { type: "WalletAccountUpdated", wallet });
    assertDeepEqual(next.wallet, wallet);
    expectPreserved(baseState, next, ["wallet"]);
  }

  {
    const loadingState = { ...baseState, loading: true };
    const next = applyDesktopEvent(loadingState, { type: "DesktopUpdateSettled" });
    assertEqual(next.loading, false);
    expectPreserved(loadingState, next, ["loading"]);
  }

  {
    const next = applyDesktopEvent(baseState, { type: "DesktopActionStarted", name: "Start" });
    assertEqual(next.loading, true);
    assertEqual(next.error, null);
    assertEqual(next.message, "Start...");
    expectPreserved(baseState, next, ["loading", "error", "message"]);
  }

  {
    const loadingState = { ...baseState, loading: true, message: "Start..." };
    const next = applyDesktopEvent(loadingState, { type: "DesktopActionCompleted", name: "Start" });
    assertEqual(next.loading, false);
    assertEqual(next.message, "Start complete");
    expectPreserved(loadingState, next, ["loading", "message"]);
  }

  {
    const loadingState = { ...baseState, loading: true, message: "Generate support package..." };
    const next = applyDesktopEvent(loadingState, {
      type: "DesktopActionCompleted",
      name: "Generate support package",
    });
    assertEqual(next.loading, false);
    assertEqual(next.message, "Generate support package complete");
    expectPreserved(loadingState, next, ["loading", "message"]);
  }

  {
    const loadingState = { ...baseState, loading: true, message: "Start..." };
    const next = applyDesktopEvent(loadingState, { type: "DesktopActionFailed", message: "Core unavailable" });
    assertEqual(next.loading, false);
    assertEqual(next.error, "Core unavailable");
    assertEqual(next.message, "Core unavailable");
    expectPreserved(loadingState, next, ["loading", "error", "message"]);
  }

  {
    const refreshingState = applyDesktopEvent(baseState, { type: "DashboardRefreshStarted" });
    const actionState = applyDesktopEvent(refreshingState, { type: "DesktopActionStarted", name: "Restart" });
    const failedState = applyDesktopEvent(actionState, { type: "DesktopActionFailed", message: "restart failed" });
    assertEqual(failedState.loading, false);
    assertEqual(failedState.error, "restart failed");
    assertEqual(failedState.message, "restart failed");
    assertEqual(failedState.mockMode, baseState.mockMode);
    assertDeepEqual(failedState.config, baseState.config);
  }

  {
    const actionState = applyDesktopEvent(baseState, { type: "DesktopActionStarted", name: "Refresh" });
    const updatedState = applyDesktopEvent(actionState, {
      type: "DashboardSnapshotUpdated",
      snapshot: baseSnapshot,
      message: "Dashboard refreshed",
      receivedAt: 1234,
    });
    const settledState = applyDesktopEvent(updatedState, { type: "DesktopUpdateSettled" });
    assertEqual(settledState.loading, false);
    assertEqual(settledState.error, null);
    assertDeepEqual(settledState.snapshot, baseSnapshot);
    assertEqual(settledState.message, "Dashboard refreshed");
  }

  {
    const explorerState = applyDesktopEvent(baseState, {
      type: "ExplorerResultUpdated",
      result: addressResult,
      message: "Address lookup complete",
    });
    const dashboardState = applyDesktopEvent(explorerState, {
      type: "DashboardSnapshotUpdated",
      snapshot: baseSnapshot,
      message: "Dashboard refreshed",
      receivedAt: 1234,
    });
    assertDeepEqual(dashboardState.explorer.result, addressResult);
    assertDeepEqual(dashboardState.snapshot, baseSnapshot);
  }
}

runDesktopReducerTransitionTests();