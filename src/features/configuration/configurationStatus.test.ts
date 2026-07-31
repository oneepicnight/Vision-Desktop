import type { DesktopState } from "../../state/desktopState";
import type { ConfigurationState as DesktopConfigurationState } from "../../types/configuration";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import type { DiagnosticsState } from "../../types/diagnostics";
import type { WalletAccountState } from "../../types/wallet";
import { deriveConfigurationViewModel } from "./configurationStatus";

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

function assertExcludes(actual: string, fragment: string, message?: string) {
  if (actual.includes(fragment)) {
    throw new Error(message ?? `Did not expect ${fragment} in ${actual}`);
  }
}

const baseConfig: NodeConfig = {
  node_name: "Configured Node",
  mode: "PrivateNetwork",
  api_port: 0,
  p2p_port: 19090,
  seed_peers: ["seed.example:19090", "peer.example:19091"],
  advertised_host: "node.example",
  advertised_port: 29090,
  mining_enabled: true,
  miner_reward_address: "abcd".padEnd(64, "0"),
  data_dir: "C:\\Users\\operator\\AppData\\Local\\Vision\\Core\\nodes\\default\\data",
  log_dir: "C:\\Users\\operator\\AppData\\Local\\Vision\\Core\\nodes\\default\\logs",
};

const baseSnapshot: DashboardSnapshot = {
  process_state: "running",
  status: {
    version: "3",
    canonical_tip_height: 42,
    canonical_tip_hash: "tip-42",
    cached_state_root_height: 42,
    cached_state_root: "root-42",
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
      blocks_found: 1,
      recovery_state: "normal",
      paused_reason: null,
    },
    recovery: {
      state: "normal",
      peer_addr: null,
      local_height: 42,
      local_work: 420,
      local_tip_hash: "tip-42",
      remote_height: null,
      remote_work: null,
      remote_tip_hash: null,
      reason: null,
    },
  },
  mining: {
    enabled: true,
    height: 42,
    difficulty: 9,
    epoch: 1,
    active: false,
    recovery_state: "normal",
    paused_reason: null,
    hash_rate_estimate: null,
  },
  peers: [
    { addr: "seed.example:19090", state: "connected", height: 42, outbound: true, height_age_secs: 2 },
    { addr: "peer.example:19091", state: "connected", height: 42, outbound: false, height_age_secs: 3 },
  ],
  api_error: null,
  core_cpu: 4,
  core_memory_bytes: 2048,
  data_dir_size_bytes: 4096,
  log_dir_size_bytes: 1024,
  mock_mode: false,
};

const baseProcess: ProcessState = {
  state: "running",
  pid: 2222,
  api_port: 18080,
  p2p_port: 19090,
  data_dir: baseConfig.data_dir,
  log_dir: baseConfig.log_dir,
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

const baseConfiguration: DesktopConfigurationState = {
  snapshot: {
    config: baseConfig,
    source_path: "C:\\Users\\operator\\AppData\\Roaming\\Vision\\Desktop\\nodes\\default.json",
    source_kind: "persisted",
  },
  appPaths: {
    desktop_config: "C:\\Users\\operator\\AppData\\Roaming\\Vision\\Desktop\\config.json",
    node_config: "C:\\Users\\operator\\AppData\\Roaming\\Vision\\Desktop\\nodes\\default.json",
    core_data: baseConfig.data_dir,
    core_logs: baseConfig.log_dir,
    desktop_logs: "C:\\Users\\operator\\AppData\\Local\\Vision\\Desktop\\logs",
    reports: "C:\\Users\\operator\\AppData\\Local\\Vision\\Desktop\\reports",
    updates: "C:\\Users\\operator\\AppData\\Local\\Vision\\Desktop\\updates",
  },
  error: null,
};

const baseState: DesktopState = {
  activeView: "configuration",
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
  lastUpdatedAt: 5_000,
};

{
  const viewModel = deriveConfigurationViewModel(baseState, 10_000);
  assertEqual(viewModel.overallStatus, "Configuration available");
  assertEqual(viewModel.sourceStatus, "Persisted Desktop node configuration");
  assertMatch(viewModel.lastRefresh, /\(5s ago\)$/);
  assertEqual(viewModel.networkEntries[1].configuredValue, "Desktop default (allocate loopback port at launch)");
}

{
  const viewModel = deriveConfigurationViewModel(
    {
      ...baseState,
      configuration: { ...baseConfiguration, snapshot: null, error: "failed to read node config: access denied" },
      process: { ...baseProcess, state: "stopped", api_port: null, p2p_port: null },
      snapshot: { ...baseSnapshot, process_state: "stopped", mining: null, status: null, peers: [] },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Configuration unavailable");
  assertMatch(viewModel.mismatchSummary, /Runtime observation unavailable/);
}

{
  const viewModel = deriveConfigurationViewModel(
    {
      ...baseState,
      configuration: { ...baseConfiguration, snapshot: null, error: "invalid node config: expected value" },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Configuration invalid");
  assertEqual(viewModel.validationState, "Invalid persisted configuration");
}

{
  const viewModel = deriveConfigurationViewModel(
    {
      ...baseState,
      mockMode: true,
      snapshot: { ...baseSnapshot, mock_mode: true },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Mock mode");
  assertEqual(viewModel.mockMode, "Yes");
}

{
  const viewModel = deriveConfigurationViewModel(
    {
      ...baseState,
      process: { ...baseProcess, p2p_port: 19091 },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Configured/runtime mismatch");
  assertMatch(viewModel.mismatchSummary, /Configured P2P port 19090 differs from runtime 19091/);
}

{
  const viewModel = deriveConfigurationViewModel(
    {
      ...baseState,
      process: null,
      snapshot: { ...baseSnapshot, process_state: "stopped", mining: null, status: null, peers: [] },
    },
    10_000,
  );
  assertEqual(viewModel.networkEntries[1].runtimeValue, "Unavailable");
  assertEqual(viewModel.networkEntries[1].runtimeSource, "Unavailable");
}

{
  const viewModel = deriveConfigurationViewModel(baseState, 10_000);
  assertEqual(viewModel.miningEntries[1].configuredValue, baseConfig.miner_reward_address);
  assertEqual(viewModel.miningEntries[1].configuredSource, "Configured");
}

{
  const configWithSecret = {
    ...baseConfig,
    private_key: "super-secret",
    seed_phrase: "twelve words",
  } as NodeConfig & { private_key: string; seed_phrase: string };
  const viewModel = deriveConfigurationViewModel(
    {
      ...baseState,
      configuration: {
        ...baseConfiguration,
        snapshot: {
          ...baseConfiguration.snapshot!,
          config: configWithSecret,
        },
      },
    },
    10_000,
  );
  const rendered = JSON.stringify(viewModel);
  assertExcludes(rendered, "super-secret");
  assertExcludes(rendered, "twelve words");
  assertExcludes(rendered, "private_key");
  assertExcludes(rendered, "seed_phrase");
}

console.log("Configuration status helper tests passed");
