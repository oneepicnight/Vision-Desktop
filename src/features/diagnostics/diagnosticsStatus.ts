import type { DesktopState } from "../../state/desktopState";

export type DiagnosticsViewModel = {
  overallStatus: string;
  summary: string;
  apiStatus: string;
  processStatus: string;
  recoveryStatus: string;
  peerSummary: string;
  binaryVerification: string;
  supportPackageStatus: string;
  lastRefresh: string;
  activeConfigPath: string;
  coreExecutablePath: string;
  dataDirectory: string;
  logDirectory: string;
  operatorMessage: string;
  apiError: string;
  manifestSummary: string;
};

function formatLastUpdated(lastUpdatedAt: number | null, now = Date.now()) {
  if (lastUpdatedAt == null) return "Unavailable";
  const ageSeconds = Math.max(0, Math.floor((now - lastUpdatedAt) / 1000));
  const time = new Date(lastUpdatedAt).toLocaleTimeString();
  if (ageSeconds < 60) return `${time} (${ageSeconds}s ago)`;
  if (ageSeconds < 3600) return `${time} (${Math.floor(ageSeconds / 60)}m ago)`;
  return `${time} (${Math.floor(ageSeconds / 3600)}h ago)`;
}

export function deriveDiagnosticsViewModel(
  state: DesktopState,
  now = Date.now(),
): DiagnosticsViewModel {
  const snapshot = state.snapshot;
  const process = state.process;
  const recovery = snapshot?.status?.recovery;
  const diagnostics = state.diagnostics;
  const verification = diagnostics.verification;
  const manifest = diagnostics.manifest;
  const processStatus = snapshot?.process_state ?? process?.state ?? "Unknown";
  const apiError = snapshot?.api_error ?? diagnostics.error ?? null;
  const peerCount = snapshot?.status?.peer_count ?? snapshot?.peers.length ?? 0;

  let overallStatus = "Unknown";
  let summary =
    "Vision Desktop is waiting for enough runtime information to describe the current Core diagnostics state.";

  if (state.mockMode || snapshot?.mock_mode) {
    overallStatus = "Mock mode";
    summary =
      "Diagnostics are showing Desktop mock data instead of a live Vision Core process.";
  } else if (snapshot == null && process == null) {
    overallStatus = "Unknown";
    summary =
      "Desktop does not yet have enough process or snapshot information to classify the Core diagnostics state.";
  } else if (processStatus !== "running") {
    overallStatus = "Unavailable";
    summary =
      "Vision Core is not currently running, so Desktop cannot confirm live diagnostics or API health.";
  } else if (apiError) {
    overallStatus = "Degraded";
    summary =
      "Vision Core is running, but Desktop could not fully refresh diagnostics from the private Core API.";
  } else if ((verification && !verification.matches) || diagnostics.error) {
    overallStatus = "Degraded";
    summary =
      "Desktop detected a diagnostics verification problem and operator review is recommended.";
  } else if (recovery && recovery.state !== "normal") {
    overallStatus = "Recovery mode";
    summary =
      "Core reported a non-normal recovery state, so some runtime behaviors may be intentionally limited.";
  } else if (snapshot?.status) {
    overallStatus = "Healthy";
    summary =
      "Desktop has a current dashboard snapshot, Core process state, and diagnostics metadata from the existing service boundary.";
  }

  return {
    overallStatus,
    summary,
    apiStatus: apiError ? "Unavailable" : snapshot ? "Connected" : "Unknown",
    processStatus,
    recoveryStatus: recovery?.state ?? "Unknown",
    peerSummary: `${peerCount} reported peer${peerCount === 1 ? "" : "s"}`,
    binaryVerification:
      verification == null
        ? "Unavailable"
        : verification.matches
          ? "Verified"
          : "Mismatch",
    supportPackageStatus:
      state.loading && state.message === "Generate support package..."
        ? "Generating"
        : "Available",
    lastRefresh: formatLastUpdated(state.lastUpdatedAt, now),
    activeConfigPath: "Not currently exposed by the Desktop service boundary",
    coreExecutablePath: verification?.binary_path ?? "Unavailable",
    dataDirectory: process?.data_dir ?? "Unavailable",
    logDirectory: process?.log_dir ?? "Unavailable",
    operatorMessage: state.message,
    apiError: apiError ?? "None",
    manifestSummary:
      manifest == null
        ? "Unavailable"
        : `${manifest.core_tag} / consensus ${manifest.consensus_tag} / protocol ${manifest.p2p_protocol_version}`,
  };
}
