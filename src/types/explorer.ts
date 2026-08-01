export type DesktopView =
  | "dashboard"
  | "wallet"
  | "marketplace"
  | "explorer"
  | "peers"
  | "mining"
  | "diagnostics"
  | "configuration";

export type ExplorerLookupMode = "address" | "transaction";

export type ExplorerAddressResult = {
  kind: "address";
  address: string;
  balance: string;
  nonce: string;
};

export type ExplorerTransactionResult = {
  kind: "transaction";
  txid: string;
  payload: string;
};

export type ExplorerResult = ExplorerAddressResult | ExplorerTransactionResult;

export type ExplorerState = {
  mode: ExplorerLookupMode;
  query: string;
  result: ExplorerResult | null;
  loading: boolean;
  error: string | null;
};
