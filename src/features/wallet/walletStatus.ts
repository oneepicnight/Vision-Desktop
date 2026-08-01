import type { DesktopState } from "../../state/desktopState";
import { resolveWalletConfiguredAddress } from "../../state/walletConfiguration";

export type WalletViewModel = {
  overallStatus: string;
  summary: string;
  configuredAddress: string;
  configuredAddressSource: string;
  liveAddress: string;
  liveAddressSource: string;
  lookupQuery: string;
  ownershipStatus: string;
  balanceAvailability: string;
  balanceValue: string;
  denominationStatus: string;
  nonceValue: string;
  coreContext: string;
  recoveryState: string;
  mockMode: string;
  lastRefresh: string;
  lookupStatus: string;
  transactionHistoryStatus: string;
};

function formatLastUpdated(lastUpdatedAt: number | null, now = Date.now()) {
  if (lastUpdatedAt == null) return "Unavailable";
  const ageSeconds = Math.max(0, Math.floor((now - lastUpdatedAt) / 1000));
  const time = new Date(lastUpdatedAt).toLocaleTimeString();
  if (ageSeconds < 60) return `${time} (${ageSeconds}s ago)`;
  if (ageSeconds < 3600) return `${time} (${Math.floor(ageSeconds / 60)}m ago)`;
  return `${time} (${Math.floor(ageSeconds / 3600)}h ago)`;
}

export function deriveWalletViewModel(
  state: DesktopState,
  now = Date.now(),
): WalletViewModel {
  const configured = resolveWalletConfiguredAddress(state.configuration.snapshot);
  const configuredAddress = configured.address;
  const hasConfiguredAddress = configuredAddress.length > 0;
  const snapshot = state.snapshot;
  const wallet = state.wallet;
  const processState = snapshot?.process_state ?? state.process?.state ?? "Unknown";
  const recoveryState = snapshot?.status?.recovery?.state ?? "Unknown";
  const isMock = state.mockMode || snapshot?.mock_mode;
  const apiError = snapshot?.api_error ?? null;

  let overallStatus = "Account data unknown";
  let summary =
    "Vision Desktop does not yet have enough confirmed account information to describe this address.";

  if (!configured.configurationAvailable) {
    overallStatus = "Configuration unavailable";
    summary =
      "Desktop could not load the persisted node configuration, so it will not guess which public account address to query.";
  } else if (isMock) {
    overallStatus = "Mock account data";
    summary =
      "Wallet information is coming from the Desktop mock lookup path rather than from a live Vision Core node.";
  } else if (!hasConfiguredAddress) {
    overallStatus = "No address configured";
    summary =
      "The current Desktop-managed node configuration does not expose a configured mining reward address.";
  } else if (processState !== "running") {
    overallStatus = "Core unavailable";
    summary =
      "Vision Core is not currently running, so Desktop cannot confirm live account data for the configured reward address.";
  } else if (apiError) {
    overallStatus = "Balance unavailable";
    summary =
      "Vision Core reported an API availability problem, so Desktop could not confirm live balance data for the configured reward address.";
  } else if (recoveryState !== "normal") {
    overallStatus = "Recovery mode";
    summary =
      "Core reported a non-normal recovery state, so account data should be interpreted with recovery context in mind.";
  } else if (wallet.account != null) {
    overallStatus = "Balance available";
    summary =
      "Desktop queried the existing read-only address lookup path and received current balance and nonce data for the configured reward address.";
  } else if (wallet.error != null) {
    overallStatus = "Balance unavailable";
    summary =
      "Desktop attempted a read-only address lookup but did not receive a usable account response.";
  } else {
    overallStatus = "Address configured but ownership unverified";
    summary =
      "Desktop has a configured reward address, but it cannot prove custody or ownership and does not yet have confirmed live account data.";
  }

  return {
    overallStatus,
    summary,
    configuredAddress: configured.displayAddress,
    configuredAddressSource: configured.source,
    liveAddress: wallet.account?.address ?? "Unavailable",
    liveAddressSource: wallet.account
      ? isMock
        ? "Desktop mock address lookup"
        : "Existing Core-compatible read-only address lookup"
      : "No live account address available",
    lookupQuery: wallet.queriedAddress ?? "Unavailable",
    ownershipStatus: hasConfiguredAddress
      ? "Unverified ownership; no custody proven"
      : "Unknown",
    balanceAvailability: wallet.account
      ? "Available"
      : wallet.error || apiError || state.configuration.error
        ? "Unavailable"
        : !configured.configurationAvailable
          ? "Unavailable"
        : hasConfiguredAddress
          ? "Unknown"
          : "Not configured",
    balanceValue: wallet.account?.balance ?? "Unavailable",
    denominationStatus: wallet.account
      ? "Unknown denomination / precision"
      : "Unavailable",
    nonceValue: wallet.account?.nonce ?? "Unavailable",
    coreContext: apiError ?? processState,
    recoveryState,
    mockMode: isMock ? "Yes" : "No",
    lastRefresh: formatLastUpdated(state.lastUpdatedAt, now),
    lookupStatus:
      wallet.error ??
      apiError ??
      state.configuration.error ??
      (wallet.account ? "Read-only lookup complete" : "No lookup result"),
    transactionHistoryStatus:
      "Not currently exposed by the Desktop service boundary",
  };
}
