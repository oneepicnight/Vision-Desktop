export type NodeMode = "LocalTesting" | "PrivateNetwork" | "InternetNetwork";

export type ProcessState = {
  state: string;
  pid?: number | null;
  api_port?: number | null;
  p2p_port?: number | null;
  data_dir: string;
  log_dir: string;
};

export type MiningStatusSnapshot = {
  available: boolean;
  active: boolean;
  blocks_found: number;
  recovery_state: string;
  paused_reason?: string | null;
};

export type RecoveryStatusSnapshot = {
  state: string;
  peer_addr?: string | null;
  local_height?: number | null;
  local_work?: number | null;
  local_tip_hash?: string | null;
  remote_height?: number | null;
  remote_work?: number | null;
  remote_tip_hash?: string | null;
  reason?: string | null;
};

export type NodeStatusSnapshot = {
  version: string;
  canonical_tip_height: number;
  canonical_tip_hash: string;
  cached_state_root_height?: number | null;
  cached_state_root?: string | null;
  mempool_size: number;
  peer_count: number;
  durable_peer_count: number;
  active_inbound_sessions: number;
  active_outbound_sessions: number;
  transient_peer_count: number;
  dialable_peer_count: number;
  mining: MiningStatusSnapshot;
  recovery: RecoveryStatusSnapshot;
};

export type MiningInfoResponse = {
  enabled: boolean;
  height: number;
  difficulty: number;
  epoch: number;
  active: boolean;
  recovery_state: string;
  paused_reason?: string | null;
  hash_rate_estimate?: number | null;
};

export type PeerEntry = {
  addr: string;
  state: string;
  height: number;
  outbound: boolean;
  height_age_secs?: number | null;
};

export type DashboardSnapshot = {
  process_state: string;
  status: NodeStatusSnapshot | null;
  mining: MiningInfoResponse | null;
  peers: PeerEntry[];
  api_error?: string | null;
  core_cpu?: number | null;
  core_memory_bytes?: number | null;
  data_dir_size_bytes: number;
  log_dir_size_bytes: number;
  mock_mode: boolean;
};

export type NodeConfig = {
  node_name: string;
  mode: NodeMode;
  api_port: number;
  p2p_port: number;
  seed_peers: string[];
  advertised_host: string | null;
  advertised_port: number | null;
  mining_enabled: boolean;
  miner_reward_address: string;
  data_dir: string;
  log_dir: string;
};
