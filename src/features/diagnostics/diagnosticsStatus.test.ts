import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import type { DiagnosticsState } from "../../types/diagnostics";
import type { DesktopState } from "../../state/desktopState";
import { deriveDiagnosticsViewModel } from "./diagnosticsStatus";

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
  miner_reward_address:
    "0000000000000000000000000000000000000000000000000000000000000000",
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
  peers: [
    {
      addr: "seed.example:19090",
      state: "connected",
      height: 12,
      outbound: true,
      height_age_secs: 2,
    },
    {
      addr: "peer.example:19091",
      state: "connected",
      height: 12,
      outbound: false,
      height_age_secs: 5,
    },
  ],
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

const baseDiagnostics: DiagnosticsState = {
  manifest: {
    core_tag: "vision-core-alpha-rc2",
    consensus_tag: "vision-core-consensus-v1.0.3",
    source_commit: "6a065df8206b50874029a27ee2b54dffae5e3cdd",
    binary_sha256: "hash",
    consensus_version: 3,
    p2p_protocol_version: 4,
    platform: "windows-x64",
  },
  verification: {
    binary_path: "C:\\Vision\\vision-core.exe",
    expected_sha256: "hash",
    actual_sha256: "hash",
    matches: true,
  },
  stdoutTail: "stdout",
  stderrTail: "",
  error: null,
};

const baseState: DesktopState = {
  activeView: "diagnostics",
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
  lastUpdatedAt: 5_000,
};

{
  const viewModel = deriveDiagnosticsViewModel(baseState, 10_000);
  assertEqual(viewModel.overallStatus, "Healthy");
  assertEqual(viewModel.apiStatus, "Connected");
  assertEqual(viewModel.binaryVerification, "Verified");
  assertEqual(viewModel.peerSummary, "2 reported peers");
  assertMatch(viewModel.lastRefresh, /\(5s ago\)$/);
}

{
  const viewModel = deriveDiagnosticsViewModel(
    {
      ...baseState,
      snapshot: { ...baseSnapshot, process_state: "stopped", status: null, mining: null, peers: [] },
      process: { ...baseProcess, state: "stopped" },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Unavailable");
  assertEqual(viewModel.processStatus, "stopped");
}

{
  const viewModel = deriveDiagnosticsViewModel(
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
  assertEqual(viewModel.overallStatus, "Recovery mode");
  assertEqual(viewModel.recoveryStatus, "recovering");
}

{
  const viewModel = deriveDiagnosticsViewModel(
    {
      ...baseState,
      mockMode: true,
      snapshot: { ...baseSnapshot, mock_mode: true },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Mock mode");
}

{
  const viewModel = deriveDiagnosticsViewModel(
    {
      ...baseState,
      snapshot: { ...baseSnapshot, api_error: "connection refused" },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Degraded");
  assertEqual(viewModel.apiError, "connection refused");
}

{
  const viewModel = deriveDiagnosticsViewModel(
    {
      ...baseState,
      snapshot: null,
      process: null,
      diagnostics: { ...baseDiagnostics, manifest: null, verification: null },
      lastUpdatedAt: null,
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Unknown");
  assertEqual(viewModel.lastRefresh, "Unavailable");
}

console.log("Diagnostics status helper tests passed");
