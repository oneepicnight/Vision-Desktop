import type { ExplorerAddressResult } from "./explorer";

export type WalletAccountState = {
  queriedAddress: string | null;
  account: ExplorerAddressResult | null;
  error: string | null;
};
