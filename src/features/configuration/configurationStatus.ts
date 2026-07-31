import type { DesktopState } from "../../state/desktopState";
import type { ConfigurationSourceKind, NodeConfigSnapshot } from "../../types/configuration";
import type { NodeConfig } from "../../types/core";

export type ConfigurationEntry = {
  label: string;
  configuredValue: string;
  configuredSource: string;
  runtimeValue: string;
  runtimeSource: string;
  note?: string;
};

export type ConfigurationViewModel = {
  overallStatus: string;
  summary: string;
  sourcePath: string;
  sourceStatus: string;
  validationState: string;
  mockMode: string;
  lastRefresh: string;
  mismatchSummary: string;
  generalEntries: ConfigurationEntry[];
  pathEntries: ConfigurationEntry[];
  networkEntries: ConfigurationEntry[];
  peerEntries: ConfigurationEntry[];
  miningEntries: ConfigurationEntry[];
  limitations: string[];
};

function formatLastUpdated(lastUpdatedAt: number | null, now = Date.now()) {
  if (lastUpdatedAt == null) return "Unavailable";
  const ageSeconds = Math.max(0, Math.floor((now - lastUpdatedAt) / 1000));
  const time = new Date(lastUpdatedAt).toLocaleTimeString();
  if (ageSeconds < 60) return `${time} (${ageSeconds}s ago)`;
  if (ageSeconds < 3600) return `${time} (${Math.floor(ageSeconds / 60)}m ago)`;
  return `${time} (${Math.floor(ageSeconds / 3600)}h ago)`;
}

function configuredSourceLabel(sourceKind: ConfigurationSourceKind | null) {
  return sourceKind === "desktop_default_created" ? "Desktop default" : "Configured";
}

function normalizeText(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed && trimmed.length > 0 ? trimmed : "Unavailable";
}

function classifyValidationState(error: string | null, hasConfig: boolean) {
  if (error?.includes("invalid node config")) {
    return "Invalid persisted configuration";
  }
  if (hasConfig) {
    return "No validation warning exposed";
  }
  if (error) {
    return "Unavailable";
  }
  return "Not currently exposed";
}

function formatConfiguredApiPort(config: NodeConfig) {
  if (config.api_port === 0) {
    return "Desktop default (allocate loopback port at launch)";
  }
  return String(config.api_port);
}

function formatAdvertisedPort(config: NodeConfig) {
  return config.advertised_port == null ? "Unavailable" : String(config.advertised_port);
}

function formatSeedPeerSummary(seedPeers: string[]) {
  if (seedPeers.length === 0) {
    return "No configured seed peers";
  }
  return `${seedPeers.length} configured seed peer${seedPeers.length === 1 ? "" : "s"}`;
}

function deriveMismatchSummary(state: DesktopState, snapshot: NodeConfigSnapshot | null) {
  if (snapshot == null) {
    return state.process?.state === "running"
      ? "Configured/runtime comparison unavailable"
      : "Runtime observation unavailable because Core is not running";
  }

  const config = snapshot.config;
  const process = state.process;
  const runtimeMining = state.snapshot?.mining ?? null;
  const mismatches: string[] = [];

  if (process?.p2p_port != null && process.p2p_port !== config.p2p_port) {
    mismatches.push(`Configured P2P port ${config.p2p_port} differs from runtime ${process.p2p_port}`);
  }
  if (process?.api_port != null && config.api_port !== 0 && process.api_port !== config.api_port) {
    mismatches.push(`Configured API port ${config.api_port} differs from runtime ${process.api_port}`);
  }
  if (process?.data_dir && process.data_dir !== config.data_dir) {
    mismatches.push("Configured data directory differs from runtime observation");
  }
  if (process?.log_dir && process.log_dir !== config.log_dir) {
    mismatches.push("Configured log directory differs from runtime observation");
  }
  if (runtimeMining != null && runtimeMining.enabled !== config.mining_enabled) {
    mismatches.push(
      `Configured mining ${config.mining_enabled ? "enabled" : "disabled"} differs from runtime ${runtimeMining.enabled ? "enabled" : "disabled"}`,
    );
  }

  if (mismatches.length === 0) {
    return process?.state === "running"
      ? "No configured/runtime mismatches detected in currently exposed fields"
      : "Runtime observation unavailable because Core is not running";
  }

  return mismatches.join("; ");
}

function createEntry(
  label: string,
  configuredValue: string,
  configuredSource: string,
  runtimeValue: string,
  runtimeSource: string,
  note?: string,
): ConfigurationEntry {
  return {
    label,
    configuredValue,
    configuredSource,
    runtimeValue,
    runtimeSource,
    note,
  };
}

export function deriveConfigurationViewModel(
  state: DesktopState,
  now = Date.now(),
): ConfigurationViewModel {
  const configuration = state.configuration;
  const snapshot = configuration.snapshot;
  const config = snapshot?.config ?? null;
  const sourceKind = snapshot?.source_kind ?? null;
  const processState = state.snapshot?.process_state ?? state.process?.state ?? "Unknown";
  const apiError = state.snapshot?.api_error ?? null;
  const isMock = state.mockMode || state.snapshot?.mock_mode;
  const mismatchSummary = deriveMismatchSummary(state, snapshot);
  const hasMismatch = mismatchSummary.startsWith("Configured ");

  let overallStatus = "Configuration unavailable";
  let summary =
    "Vision Desktop does not currently have enough confirmed information to display the Desktop-managed node configuration.";

  if (isMock) {
    overallStatus = "Mock mode";
    summary =
      "Configuration values come from the Desktop-managed configuration path, but runtime observations are currently from the Desktop mock snapshot rather than a live Core process.";
  } else if (configuration.error?.includes("invalid node config")) {
    overallStatus = "Configuration invalid";
    summary =
      "Desktop could not parse the persisted node configuration. Runtime state, if any, should be treated separately from this configuration error.";
  } else if (snapshot && hasMismatch) {
    overallStatus = "Configured/runtime mismatch";
    summary =
      "Desktop loaded a node configuration and also has live runtime observations, but at least one currently exposed field differs.";
  } else if (snapshot) {
    overallStatus = processState === "running" ? "Configuration available" : "Configuration loaded";
    summary =
      "Desktop loaded the current node configuration and is showing only confirmed runtime observations that are already exposed by the existing process and snapshot models.";
  } else if (configuration.error) {
    overallStatus = "Configuration unavailable";
    summary =
      "Desktop could not load the current node configuration, but this does not by itself prove that Vision Core is invalid or unavailable.";
  }

  const sourcePath = snapshot?.source_path ?? configuration.appPaths?.node_config ?? "Unavailable";
  const sourceStatus = snapshot
    ? snapshot.source_kind === "desktop_default_created"
      ? "Desktop default config created because no saved node config existed"
      : "Persisted Desktop node configuration"
    : configuration.appPaths?.node_config
      ? "Expected Desktop node configuration path"
      : "Unavailable";

  const configuredLabel = configuredSourceLabel(sourceKind);
  const runtimeApiSource =
    state.process?.api_port != null
      ? "Runtime observed"
      : processState === "running"
        ? "Runtime value not exposed"
        : "Unavailable";
  const runtimeP2pSource = state.process?.p2p_port != null ? "Runtime observed" : "Unavailable";
  const runtimePathSource = processState === "running" ? "Runtime observed" : "Unavailable";
  const runtimeMiningSource = state.snapshot?.mining != null ? "Runtime observed" : "Unavailable";
  const runtimeNotExposed = "Not exposed";

  const generalEntries: ConfigurationEntry[] = [
    createEntry(
      "Node name",
      config?.node_name ?? "Unavailable",
      config ? configuredLabel : "Unavailable",
      "Runtime value not exposed",
      runtimeNotExposed,
    ),
    createEntry(
      "Configuration mode",
      config?.mode ?? "Unavailable",
      config ? configuredLabel : "Unavailable",
      "Runtime value not exposed",
      runtimeNotExposed,
    ),
    createEntry(
      "Process state",
      "Not applicable",
      "Not applicable",
      processState,
      state.process || state.snapshot ? "Runtime observed" : "Unavailable",
      apiError ? `Current API error: ${apiError}` : undefined,
    ),
  ];

  const pathEntries: ConfigurationEntry[] = [
    createEntry(
      "Node config path",
      sourcePath,
      snapshot ? "Desktop config source" : configuration.appPaths?.node_config ? "Desktop default path" : "Unavailable",
      "Not applicable",
      "Not applicable",
    ),
    createEntry(
      "Data directory",
      config?.data_dir ?? "Unavailable",
      config ? configuredLabel : "Unavailable",
      state.process?.data_dir ?? "Unavailable",
      runtimePathSource,
    ),
    createEntry(
      "Log directory",
      config?.log_dir ?? "Unavailable",
      config ? configuredLabel : "Unavailable",
      state.process?.log_dir ?? "Unavailable",
      runtimePathSource,
    ),
  ];

  const networkEntries: ConfigurationEntry[] = [
    createEntry(
      "API bind host",
      "Not exposed by current Desktop config model",
      "Not exposed",
      "Not exposed by current runtime snapshot",
      runtimeNotExposed,
    ),
    createEntry(
      "API port",
      config ? formatConfiguredApiPort(config) : "Unavailable",
      config ? configuredLabel : "Unavailable",
      state.process?.api_port != null ? String(state.process.api_port) : "Unavailable",
      runtimeApiSource,
      config?.api_port === 0 ? "A configured API port of 0 means Desktop will request a loopback port at launch." : undefined,
    ),
    createEntry(
      "P2P port",
      config ? String(config.p2p_port) : "Unavailable",
      config ? configuredLabel : "Unavailable",
      state.process?.p2p_port != null ? String(state.process.p2p_port) : "Unavailable",
      runtimeP2pSource,
    ),
    createEntry(
      "Advertised host",
      normalizeText(config?.advertised_host),
      config ? configuredLabel : "Unavailable",
      "Runtime value not exposed",
      runtimeNotExposed,
    ),
    createEntry(
      "Advertised port",
      config ? formatAdvertisedPort(config) : "Unavailable",
      config ? configuredLabel : "Unavailable",
      "Runtime value not exposed",
      runtimeNotExposed,
    ),
  ];

  const peerEntries: ConfigurationEntry[] = [
    createEntry(
      "Seed peer summary",
      config ? formatSeedPeerSummary(config.seed_peers) : "Unavailable",
      config ? configuredLabel : "Unavailable",
      `${state.snapshot?.peers.length ?? 0} runtime peer${state.snapshot?.peers.length === 1 ? "" : "s"} observed`,
      state.snapshot ? "Runtime observed" : "Unavailable",
    ),
    createEntry(
      "Configured seed peers",
      config && config.seed_peers.length > 0 ? config.seed_peers.join(", ") : "None",
      config ? configuredLabel : "Unavailable",
      "Runtime peer list is separate from configured seeds",
      runtimeNotExposed,
    ),
    createEntry(
      "Private-peer policy",
      "Not exposed by current Desktop config model",
      "Not exposed",
      "Not exposed by current runtime snapshot",
      runtimeNotExposed,
    ),
  ];

  const miningEntries: ConfigurationEntry[] = [
    createEntry(
      "Mining enabled",
      config == null ? "Unavailable" : config.mining_enabled ? "Enabled" : "Disabled",
      config ? configuredLabel : "Unavailable",
      state.snapshot?.mining == null ? "Unavailable" : state.snapshot.mining.enabled ? "Enabled" : "Disabled",
      runtimeMiningSource,
    ),
    createEntry(
      "Mining reward address",
      config?.miner_reward_address ?? "Unavailable",
      config ? configuredLabel : "Unavailable",
      "Runtime value not exposed",
      runtimeNotExposed,
      "This is treated as a public configured value, not as proof of custody or ownership.",
    ),
  ];

  return {
    overallStatus,
    summary,
    sourcePath,
    sourceStatus,
    validationState: classifyValidationState(configuration.error, snapshot != null),
    mockMode: isMock ? "Yes" : "No",
    lastRefresh: formatLastUpdated(state.lastUpdatedAt, now),
    mismatchSummary,
    generalEntries,
    pathEntries,
    networkEntries,
    peerEntries,
    miningEntries,
    limitations: [
      "This page is read-only and does not apply configuration changes.",
      "Configured values do not prove that the running Core process is using them unless a matching runtime observation is exposed.",
      "API bind host and private-peer policy are not currently exposed by the Desktop config model.",
      "Secret-bearing values are deliberately excluded from this page.",
    ],
  };
}
