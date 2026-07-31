import type { DashboardSnapshot, NodeConfig, ProcessState } from "../../types/core";
import type { ConfigurationState } from "../../types/configuration";
import type { DiagnosticsState } from "../../types/diagnostics";
import type { ExplorerAddressResult } from "../../types/explorer";
import type { WalletAccountState } from "../../types/wallet";
import type { DesktopState } from "../../state/desktopState";
import { deriveWalletViewModel } from "./walletStatus";

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
  mining_enabled: true,
  miner_reward_address: "abcd".padEnd(64, "0"),
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
    enabled: true,
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

const baseDiagnostics: DiagnosticsState = {
  manifest: null,
  verification: null,
  stdoutTail: null,
  stderrTail: null,
  error: null,
};

const baseWallet: WalletAccountState = {
  queriedAddress: baseConfig.miner_reward_address,
  account: null,
  error: null,
};

const baseConfiguration: ConfigurationState = {
  snapshot: null,
  appPaths: null,
  error: null,
};

const liveAccount: ExplorerAddressResult = {
  kind: "address",
  address: baseConfig.miner_reward_address,
  balance: "300003",
  nonce: "3",
};

const baseState: DesktopState = {
  activeView: "wallet",
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
  const viewModel = deriveWalletViewModel(
    {
      ...baseState,
      config: { ...baseConfig, miner_reward_address: "" },
      wallet: { queriedAddress: null, account: null, error: null },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "No address configured");
  assertEqual(viewModel.configuredAddress, "Unavailable");
}

{
  const viewModel = deriveWalletViewModel(baseState, 10_000);
  assertEqual(viewModel.overallStatus, "Address configured but ownership unverified");
  assertEqual(viewModel.ownershipStatus, "Unverified ownership; no custody proven");
}

{
  const viewModel = deriveWalletViewModel(
    { ...baseState, snapshot: { ...baseSnapshot, process_state: "stopped" }, process: { ...baseProcess, state: "stopped" } },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Core unavailable");
}

{
  const viewModel = deriveWalletViewModel(
    {
      ...baseState,
      snapshot: { ...baseSnapshot, api_error: "connection refused" },
    },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Balance unavailable");
  assertEqual(viewModel.lookupStatus, "connection refused");
}

{
  const viewModel = deriveWalletViewModel(
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
}

{
  const viewModel = deriveWalletViewModel(
    { ...baseState, mockMode: true, snapshot: { ...baseSnapshot, mock_mode: true } },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Mock account data");
}

{
  const viewModel = deriveWalletViewModel(
    { ...baseState, wallet: { queriedAddress: baseConfig.miner_reward_address, account: liveAccount, error: null } },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Balance available");
  assertEqual(viewModel.balanceValue, "300003");
  assertEqual(viewModel.nonceValue, "3");
  assertEqual(viewModel.balanceAvailability, "Available");
}

{
  const viewModel = deriveWalletViewModel(
    { ...baseState, wallet: { queriedAddress: baseConfig.miner_reward_address, account: null, error: "lookup failed" } },
    10_000,
  );
  assertEqual(viewModel.overallStatus, "Balance unavailable");
  assertEqual(viewModel.lookupStatus, "lookup failed");
}

{
  const viewModel = deriveWalletViewModel(
    { ...baseState, wallet: { queriedAddress: baseConfig.miner_reward_address, account: liveAccount, error: null } },
    10_000,
  );
  assertEqual(viewModel.denominationStatus, "Unknown denomination / precision");
}

{
  const viewModel = deriveWalletViewModel(
    { ...baseState, wallet: { queriedAddress: baseConfig.miner_reward_address, account: liveAccount, error: null } },
    10_000,
  );
  assertMatch(viewModel.lastRefresh, /\(5s ago\)$/);
}

console.log("Wallet status helper tests passed");