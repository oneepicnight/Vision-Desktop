import type { DesktopState } from "../../state/desktopState";
import { shortHash } from "../../utils/format";

export type MiningViewModel = {
  headline: string;
  detail: string;
  runtimeEnabled: string;
  activity: string;
  availability: string;
  processReadiness: string;
  heightContext: string;
  recoveryState: string;
  pausedReason: string;
  rewardAddress: string;
  lastUpdated: string;
};

function formatLastUpdated(lastUpdatedAt: number | null, now = Date.now()) {
  if (lastUpdatedAt == null) return "Unavailable";
  const ageSeconds = Math.max(0, Math.floor((now - lastUpdatedAt) / 1000));
  const time = new Date(lastUpdatedAt).toLocaleTimeString();
  if (ageSeconds < 60) return `${time} (${ageSeconds}s ago)`;
  if (ageSeconds < 3600) return `${time} (${Math.floor(ageSeconds / 60)}m ago)`;
  return `${time} (${Math.floor(ageSeconds / 3600)}h ago)`;
}

export function deriveMiningViewModel(
  state: DesktopState,
  now = Date.now(),
): MiningViewModel {
  const runtimeMining = state.snapshot?.mining ?? null;
  const statusMining = state.snapshot?.status?.mining ?? null;
  const recovery = state.snapshot?.status?.recovery ?? null;
  const processState = state.snapshot?.process_state ?? state.process?.state ?? "Unknown";
  const pausedReason = runtimeMining?.paused_reason ?? statusMining?.paused_reason ?? null;
  const recoveryState =
    runtimeMining?.recovery_state ??
    statusMining?.recovery_state ??
    recovery?.state ??
    "Unknown";

  let headline = "Mining status unknown";
  let detail =
    "Vision Desktop is waiting for enough runtime data to describe the miner state.";

  if (state.mockMode || state.snapshot?.mock_mode) {
    headline = "Mock mining data";
    detail =
      "Mining information is coming from the Desktop mock snapshot, not from a live Core instance.";
  } else if (processState !== "running") {
    headline = "Core unavailable";
    detail =
      "Vision Core is not currently running, so Desktop cannot verify live mining activity.";
  } else if (!state.config.mining_enabled) {
    headline = "Mining disabled by Desktop configuration";
    detail =
      "The Desktop-managed node configuration currently has mining turned off.";
  } else if (pausedReason) {
    headline = "Mining paused";
    detail = `Core reported a pause reason: ${pausedReason}`;
  } else if (recoveryState !== "normal") {
    headline = "Mining blocked by recovery state";
    detail =
      "Core reported a non-normal recovery state, so normal mining activity may be intentionally suppressed.";
  } else if (statusMining?.available === false) {
    headline = "Mining unavailable";
    detail =
      "Core reported that mining is not currently available for this node.";
  } else if (runtimeMining?.enabled === true && runtimeMining.active) {
    headline = "Mining active";
    detail =
      "Core reported that mining is enabled and currently active.";
  } else if (runtimeMining?.enabled === true && runtimeMining.active === false) {
    headline = "Mining enabled but idle";
    detail =
      "Core reported that mining is enabled, but it is not currently active.";
  } else if (runtimeMining?.enabled === false) {
    headline = "Mining disabled by Core";
    detail =
      "Core reported that mining is currently disabled at runtime.";
  }

  return {
    headline,
    detail,
    runtimeEnabled:
      runtimeMining == null ? "Unknown" : runtimeMining.enabled ? "Enabled" : "Disabled",
    activity:
      runtimeMining?.active ?? statusMining?.active
        ? "Active"
        : runtimeMining != null || statusMining != null
          ? "Inactive"
          : "Unknown",
    availability:
      statusMining == null
        ? "Unknown"
        : statusMining.available
          ? "Available"
          : "Unavailable",
    processReadiness: processState,
    heightContext: String(
      runtimeMining?.height ??
        state.snapshot?.status?.canonical_tip_height ??
        "Unavailable",
    ),
    recoveryState,
    pausedReason: pausedReason ?? "None",
    rewardAddress:
      state.config.miner_reward_address || "Unavailable",
    lastUpdated: formatLastUpdated(state.lastUpdatedAt, now),
  };
}
