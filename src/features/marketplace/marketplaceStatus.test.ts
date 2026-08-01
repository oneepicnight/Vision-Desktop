import type { DesktopState } from "../../state/desktopState";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import { deriveMarketplaceViewModel } from "./marketplaceStatus";

function assertEqual<T>(actual: T, expected: T, message?: string) {
  if (actual !== expected) {
    throw new Error(message ?? `Expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertMatch(actual: string, expected: RegExp, message?: string) {
  if (!expected.test(actual)) {
    throw new Error(message ?? `Expected ${actual} to match ${String(expected)}`);
  }
}

const baseConfig: NodeConfig = {
  node_name: "Default Node",
  mode: "LocalTesting",
  api_port: 18080,
  p2p_port: 19090,
  seed_peers: [],
  advertised_host: null,
  advertised_port: null,
  mining_enabled: false,
  miner_reward_address: "",
  data_dir: "data",
  log_dir: "logs",
};

const baseSnapshot: DashboardSnapshot = {
  process_state: "running",
  status: {
    version: "3",
    canonical_tip_height: 12,
    canonical_tip_hash: "tip-12",
    cached_state_root_height: 12,
    cached_state_root: "root-12",
    mempool_size: 0,
    peer_count: 2,
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
      local_height: 12,
      local_work: 120,
      local_tip_hash: "tip-12",
      remote_height: null,
      remote_work: null,
      remote_tip_hash: null,
      reason: null,
    },
  },
  mining: {
    enabled: false,
    height: 12,
    difficulty: 2,
    epoch: 0,
    active: false,
    recovery_state: "normal",
    paused_reason: null,
    hash_rate_estimate: null,
  },
  peers: [],
  api_error: null,
  core_cpu: 5,
  core_memory_bytes: 2048,
  data_dir_size_bytes: 4096,
  log_dir_size_bytes: 1024,
  mock_mode: false,
};

const baseProcess: ProcessState = {
  state: "running",
  pid: 1010,
  api_port: 18080,
  p2p_port: 19090,
  data_dir: "data",
  log_dir: "logs",
};

const baseState: DesktopState = {
  activeView: "marketplace",
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
  diagnostics: {
    manifest: null,
    verification: null,
    stdoutTail: null,
    stderrTail: null,
    error: null,
  },
  configuration: {
    snapshot: null,
    appPaths: null,
    error: null,
  },
  wallet: {
    queriedAddress: null,
    account: null,
    error: null,
  },
  lastUpdatedAt: 5_000,
  activeLifecycleAction: null,
  pendingLifecycleConfirmation: null,
};

{
  const viewModel = deriveMarketplaceViewModel(baseState, 10_000);
  assertEqual(viewModel.headline, "Marketplace API not exposed");
  assertEqual(viewModel.marketDataStatus, "Not connected");
  assertEqual(viewModel.actionStatus, "Unavailable; no trade or checkout commands");
}

{
  const viewModel = deriveMarketplaceViewModel(
    { ...baseState, mockMode: true, snapshot: { ...baseSnapshot, mock_mode: true } },
    10_000,
  );
  assertEqual(viewModel.headline, "Marketplace preview only");
  assertEqual(viewModel.mockMode, "Yes");
}

{
  const viewModel = deriveMarketplaceViewModel(
    {
      ...baseState,
      snapshot: { ...baseSnapshot, process_state: "stopped" },
      process: { ...baseProcess, state: "stopped" },
    },
    10_000,
  );
  assertEqual(viewModel.headline, "Core unavailable");
}

{
  const viewModel = deriveMarketplaceViewModel(
    { ...baseState, snapshot: { ...baseSnapshot, api_error: "connection refused" } },
    10_000,
  );
  assertEqual(viewModel.headline, "Core API unavailable");
  assertEqual(viewModel.coreContext, "connection refused");
}

{
  const viewModel = deriveMarketplaceViewModel(
    {
      ...baseState,
      snapshot: {
        ...baseSnapshot,
        status: {
          ...baseSnapshot.status!,
          recovery: { ...baseSnapshot.status!.recovery, state: "recovering" },
        },
      },
    },
    10_000,
  );
  assertEqual(viewModel.headline, "Recovery mode");
  assertEqual(viewModel.recoveryState, "recovering");
}

{
  const viewModel = deriveMarketplaceViewModel(baseState, 10_000);
  assertMatch(viewModel.lastRefresh, /\(5s ago\)$/);
}

console.log("Marketplace status helper tests passed");
