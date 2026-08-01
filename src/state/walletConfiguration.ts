import type { NodeConfigSnapshot } from "../types/configuration";

export type WalletConfiguredAddress = {
  address: string;
  displayAddress: string;
  source: string;
  configurationAvailable: boolean;
};

export function resolveWalletConfiguredAddress(
  snapshot: NodeConfigSnapshot | null,
): WalletConfiguredAddress {
  if (snapshot == null) {
    return {
      address: "",
      displayAddress: "Unavailable",
      source: "Desktop node configuration unavailable",
      configurationAvailable: false,
    };
  }

  const address = snapshot.config.miner_reward_address.trim();
  const source =
    snapshot.source_kind === "persisted"
      ? "Persisted Desktop node configuration"
      : "Desktop default node configuration";

  return {
    address,
    displayAddress: address || "Unavailable",
    source: address ? source : `${source}; no reward address configured`,
    configurationAvailable: true,
  };
}
