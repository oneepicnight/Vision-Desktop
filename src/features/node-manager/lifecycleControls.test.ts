import type { DesktopState } from "../../state/desktopState";
import type { ConfigurationState } from "../../types/configuration";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import type { DiagnosticsState } from "../../types/diagnostics";
import type { WalletAccountState } from "../../types/wallet";
import {
  canRestartCore,
  canStartCore,
  canStopCore,
  deriveLifecycleControls,
} from "./lifecycleControls";

function assertEqual<T>(actual: T, expected: T, message?: string) {
  if (actual !== expected) {
    throw new Error(message ?? `Expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertMatch(actual: string, pattern: RegExp, message?: string) {
  if (!pattern.test(actual)) {
    throw new Error(message ?? `Expected ${actual} to match ${pattern}`);
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
  data_dir: "data",
  log_dir: "logs",
};

const baseSnapshot: DashboardSnapshot = {
  process_state: "stopped",
  status: {
    version: "3",
    canonical_tip_height: 10,
    canonical_tip_hash: "tip-a",
    cached_state_root_height: 10,
    cached_state_root: "root-a",
    mempool_size: 0,
    peer_count: 1,
    durable_peer_count: 1,
    active_inbound_sessions: 0,
    active_outbound_sessions: 1,
    transient_peer_count: 0,
    dialable_peer_count: 1,
    mining: {
      available: true,
      active: false,
      blocks_found: 0,
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
    enabled: false,
    height: 10,
    difficulty: 1,
    epoch: 0,
    active: false,
    recovery_state: "normal",
    paused_reason: null,
    hash_rate_estimate: null,
  },
  peers: [],
  api_error: "Core is not running",
  core_cpu: null,
  core_memory_bytes: null,
  data_dir_size_bytes: 1024,
  log_dir_size_bytes: 256,
  mock_mode: false,
};

const baseProcess: ProcessState = {
  state: "stopped",
  pid: null,
  api_port: null,
  p2p_port: null,
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

const baseConfiguration: ConfigurationState = {
  snapshot: null,
  appPaths: null,
  error: null,
};

const baseWallet: WalletAccountState = {
  queriedAddress: null,
  account: null,
  error: null,
};

const baseState: DesktopState = {
  activeView: "dashboard",
  mockMode: false,
  snapshot: baseSnapshot,
  process: baseProcess,
  message: "Ready",
  loading: false,
  error: null,
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

{
  assertEqual(canStartCore(baseState), true);
  assertEqual(canStopCore(baseState), false);
  assertEqual(canRestartCore(baseState), false);
}

{
  const runningState = {
    ...baseState,
    snapshot: { ...baseSnapshot, process_state: "running", api_error: null },
    process: { ...baseProcess, state: "running", pid: 1234, api_port: 18080, p2p_port: 19090 },
  };
  assertEqual(canStartCore(runningState), false);
  assertEqual(canStopCore(runningState), true);
  assertEqual(canRestartCore(runningState), true);
}

{
  const crashedState = {
    ...baseState,
    snapshot: { ...baseSnapshot, process_state: "crashed", api_error: "process exited" },
    process: { ...baseProcess, state: "crashed", pid: 1234, api_port: 18080, p2p_port: 19090 },
  };
  assertEqual(canStartCore(crashedState), true);
  assertEqual(canStopCore(crashedState), true);
  assertEqual(canRestartCore(crashedState), true);
}

{
  const inflightState = { ...baseState, activeLifecycleAction: "start" as const };
  const viewModel = deriveLifecycleControls(inflightState);
  assertEqual(viewModel.start.enabled, false);
  assertEqual(viewModel.stop.enabled, false);
  assertEqual(viewModel.restart.enabled, false);
  assertEqual(viewModel.refreshEnabled, false);
  assertMatch(viewModel.progressMessage ?? "", /Start command in progress/);
}

{
  const mockState = { ...baseState, mockMode: true, snapshot: { ...baseSnapshot, mock_mode: true } };
  const viewModel = deriveLifecycleControls(mockState);
  assertEqual(viewModel.start.enabled, false);
  assertEqual(viewModel.stop.reason, "Lifecycle controls are disabled in mock mode.");
}

{
  const restartState = {
    ...baseState,
    snapshot: { ...baseSnapshot, process_state: "running", api_error: null },
    process: { ...baseProcess, state: "running", pid: 1234, api_port: 18080, p2p_port: 19090 },
    pendingLifecycleConfirmation: "restart" as const,
  };
  const viewModel = deriveLifecycleControls(restartState);
  assertEqual(viewModel.confirmationTitle, "Confirm restart");
  assertMatch(viewModel.confirmationBody ?? "", /stopped and started again/);
}

{
  const recoveryState = {
    ...baseState,
    snapshot: {
      ...baseSnapshot,
      status: {
        ...baseSnapshot.status!,
        recovery: { ...baseSnapshot.status!.recovery, state: "recovering" },
      },
    },
  };
  const viewModel = deriveLifecycleControls(recoveryState);
  assertMatch(viewModel.recoveryNote ?? "", /not disabled by recovery state alone/);
}

{
  const unknownState = { ...baseState, snapshot: null, process: null };
  const viewModel = deriveLifecycleControls(unknownState);
  assertEqual(viewModel.start.enabled, false);
  assertEqual(viewModel.stop.enabled, false);
  assertEqual(viewModel.restart.enabled, false);
  assertEqual(viewModel.start.reason, "Current process state is unknown.");
}

console.log("Lifecycle controls helper tests passed");