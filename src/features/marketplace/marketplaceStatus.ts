import type { DesktopState } from "../../state/desktopState";

export type MarketplaceViewModel = {
  headline: string;
  summary: string;
  coreContext: string;
  recoveryState: string;
  marketDataStatus: string;
  integrationStatus: string;
  actionStatus: string;
  mockMode: string;
  lastRefresh: string;
};

function formatLastUpdated(lastUpdatedAt: number | null, now = Date.now()) {
  if (lastUpdatedAt == null) return "Unavailable";
  const ageSeconds = Math.max(0, Math.floor((now - lastUpdatedAt) / 1000));
  const time = new Date(lastUpdatedAt).toLocaleTimeString();
  if (ageSeconds < 60) return `${time} (${ageSeconds}s ago)`;
  if (ageSeconds < 3600) return `${time} (${Math.floor(ageSeconds / 60)}m ago)`;
  return `${time} (${Math.floor(ageSeconds / 3600)}h ago)`;
}

export function deriveMarketplaceViewModel(
  state: DesktopState,
  now = Date.now(),
): MarketplaceViewModel {
  const snapshot = state.snapshot;
  const isMock = state.mockMode || snapshot?.mock_mode === true;
  const processState = snapshot?.process_state ?? state.process?.state ?? "unknown";
  const apiError = snapshot?.api_error ?? null;
  const recoveryState = snapshot?.status?.recovery?.state ?? "unknown";

  let headline = "Marketplace API not exposed";
  let summary =
    "Vision Desktop has no approved marketplace data or transaction service boundary, so no market facts or trading controls are displayed.";

  if (isMock) {
    headline = "Marketplace preview only";
    summary =
      "This screen demonstrates the intended operator layout. Mock mode does not provide fabricated prices, listings, orders, balances, or trade history.";
  } else if (processState !== "running") {
    headline = "Core unavailable";
    summary =
      "Vision Core is not currently observed as running. Marketplace integration remains unavailable and no market request is attempted.";
  } else if (apiError) {
    headline = "Core API unavailable";
    summary =
      "The current Desktop snapshot reports an API problem. Marketplace integration remains unavailable and the error is not treated as market data.";
  } else if (recoveryState !== "normal") {
    headline = "Recovery mode";
    summary =
      "Core reports a non-normal recovery state. Marketplace integration remains read-only and disconnected while recovery context is visible.";
  }

  return {
    headline,
    summary,
    coreContext: apiError ?? processState,
    recoveryState,
    marketDataStatus: "Not connected",
    integrationStatus: "Approved Desktop service boundary required",
    actionStatus: "Unavailable; no trade or checkout commands",
    mockMode: isMock ? "Yes" : "No",
    lastRefresh: formatLastUpdated(state.lastUpdatedAt, now),
  };
}
