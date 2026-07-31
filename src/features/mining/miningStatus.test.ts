import { deriveMiningViewModel } from "./miningStatus";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import type { ConfigurationState } from "../../types/configuration";
import type { DiagnosticsState } from "../../types/diagnostics";
import type { WalletAccountState } from "../../types/wallet";
import type { DesktopState } from "../../state/desktopState";

function assertEqual<T>(actual: T, expected: T, message?: string) {
  if (actual !== expected) {
    throw new Error(message ?? `Expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertMatch(actual: string, pattern: RegExp, message?: string) {
  if (!pattern.test(actual)) {
    throw new Error(message ?? `Expected ${actual} to match ${pattern.toString()}`);
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
  mining_enabled: true,
  miner_reward_address: "abcd".padEnd(64, "0"),
  data_dir: "",
  log_dir: "",
};

const baseSnapshot: DashboardSnapshot = {
  process_state: "running",
  status: {
    version: "3",
    canonical_tip_height: 25,
    canonical_tip_hash: "tip-a",
    cached_state_root_height: 25,
    cached_state_root: "root-a",
    mempool_size: 0,
    peer_count: 2,
    durable_peer_count: 2,
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
      local_height: 25,
      local_work: 1000,
      local_tip_hash: "tip-a",
      remote_height: null,
      remote_work: null,
      remote_tip_hash: null,
      reason: null,
    },
  },
  mining: {
    enabled: true,
    height: 25,
    difficulty: 3,
    epoch: 0,
    active: false,
    recovery_state: "normal",
    paused_reason: null,
    hash_rate_estimate: null,
  },
  peers: [],
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
  lastUpdatedAt: Date.UTC(2026, 6, 31, 12, 0, 0),
  activeLifecycleAction: null,
  pendingLifecycleConfirmation: null,
};

{
  const viewModel = deriveMiningViewModel(baseState, Date.UTC(2026, 6, 31, 12, 0, 45));
  assertEqual(viewModel.headline, "Mining enabled but idle");
  assertEqual(viewModel.runtimeEnabled, "Enabled");
  assertEqual(viewModel.activity, "Inactive");
}

{
  const viewModel = deriveMiningViewModel(
    { ...baseState, config: { ...baseConfig, mining_enabled: false } },
    Date.UTC(2026, 6, 31, 12, 0, 45),
  );
  assertEqual(viewModel.headline, "Mining disabled by Desktop configuration");
}

{
  const viewModel = deriveMiningViewModel(
    {
      ...baseState,
      snapshot: {
        ...baseSnapshot,
        mining: { ...baseSnapshot.mining!, paused_reason: "higher-work recovery", active: false },
      },
    },
    Date.UTC(2026, 6, 31, 12, 0, 45),
  );
  assertEqual(viewModel.headline, "Mining paused");
  assertMatch(viewModel.detail, /higher-work recovery/);
}

{
  const viewModel = deriveMiningViewModel(
    {
      ...baseState,
      snapshot: {
        ...baseSnapshot,
        mining: { ...baseSnapshot.mining!, recovery_state: "catching_up" },
        status: {
          ...baseSnapshot.status!,
          recovery: { ...baseSnapshot.status!.recovery, state: "catching_up" },
        },
      },
    },
    Date.UTC(2026, 6, 31, 12, 0, 45),
  );
  assertEqual(viewModel.headline, "Mining blocked by recovery state");
}

{
  const viewModel = deriveMiningViewModel(
    {
      ...baseState,
      snapshot: {
        ...baseSnapshot,
        status: {
          ...baseSnapshot.status!,
          mining: { ...baseSnapshot.status!.mining, available: false },
        },
      },
    },
    Date.UTC(2026, 6, 31, 12, 0, 45),
  );
  assertEqual(viewModel.headline, "Mining unavailable");
  assertEqual(viewModel.availability, "Unavailable");
}

{
  const viewModel = deriveMiningViewModel(
    { ...baseState, snapshot: { ...baseSnapshot, process_state: "stopped" } },
    Date.UTC(2026, 6, 31, 12, 0, 45),
  );
  assertEqual(viewModel.headline, "Core unavailable");
}

{
  const viewModel = deriveMiningViewModel(
    { ...baseState, mockMode: true, snapshot: { ...baseSnapshot, mock_mode: true } },
    Date.UTC(2026, 6, 31, 12, 0, 45),
  );
  assertEqual(viewModel.headline, "Mock mining data");
}

console.log("Mining status helper tests passed");