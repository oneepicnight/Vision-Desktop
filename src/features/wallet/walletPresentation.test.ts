import {
  WalletPresentationEpoch,
  pendingReconciliationMessage,
  spendingControlsEnabled,
  validTransferDraft,
  validWalletIdentity,
  walletErrorCode,
  walletErrorMessage,
} from "./walletPresentation";

function assert(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}

assert(validWalletIdentity("operator_1", "Operator Wallet"), "valid public identity is accepted");
assert(!validWalletIdentity("operator 1", "Operator Wallet"), "spaces in identifiers are rejected");
assert(!validWalletIdentity("operator", " padded"), "padded labels are rejected");
assert(!validWalletIdentity("a".repeat(65), "Wallet"), "oversized identifiers are rejected");

assert(validTransferDraft("a".repeat(64), "12.000000001"), "canonical transfer is accepted");
assert(!validTransferDraft("A".repeat(64), "1"), "uppercase addresses are rejected");
assert(!validTransferDraft("a".repeat(64), "1e9"), "exponent amounts are rejected");
assert(!validTransferDraft("a".repeat(64), "01"), "noncanonical amounts are rejected");
assert(!validTransferDraft("a".repeat(64), "0"), "zero-value transfers are rejected");
assert(!validTransferDraft("a".repeat(64), "0.0000000001"), "more than nine decimals are rejected");

assert(walletErrorCode({ code: "insufficient_balance" }) === "insufficient_balance", "fixed codes pass");
assert(walletErrorCode({ code: "secret-value" }) === "wallet_unavailable", "unknown codes are hidden");
assert(walletErrorMessage(new Error("C:\\private\\vault")) === "The wallet is unavailable.", "dependency errors are not rendered");

const unlocked = { vault_exists: true, locked: false, account: null };
const clearActivity = { records: [], history_complete: false as const, pending_reconciliation: null };
assert(spendingControlsEnabled(unlocked, clearActivity, false), "unlocked reconciled wallet may spend");
assert(!spendingControlsEnabled(unlocked, null, false), "unknown reconciliation blocks spending");
assert(!spendingControlsEnabled(unlocked, { ...clearActivity, pending_reconciliation: { state: "outcome_unknown", transaction_id: "1".repeat(64) } }, false), "pending reconciliation blocks spending");

const presentationEpoch = new WalletPresentationEpoch();
const firstRequest = presentationEpoch.capture();
assert(presentationEpoch.isCurrent(firstRequest), "fresh completion may update public state");
presentationEpoch.invalidate();
assert(!presentationEpoch.isCurrent(firstRequest), "lifecycle clearing rejects stale completion");
const secondRequest = presentationEpoch.capture();
assert(presentationEpoch.isCurrent(secondRequest), "new work uses the advanced epoch");

assert(
  pendingReconciliationMessage({ state: "accepted_recording_pending", transaction_id: "1".repeat(64) })?.includes("Do not resubmit"),
  "accepted recording warnings prohibit resubmission",
);
