import {
  WalletPresentationEpoch,
  pendingReconciliationMessage,
  runWalletNativeModal,
  runWalletNativeSelectionWorkflow,
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

async function nativeSelectionThenLifecycleContinues(operationName: "create" | "restore") {
  const tracker = new WalletPresentationEpoch();
  const epoch = tracker.capture();
  let lifecycleInvocations = 0;
  const lifecycle = await runWalletNativeSelectionWorkflow(
    tracker,
    epoch,
    async () => {
      assert(tracker.observeWindowBlur(), "the operation-owned native dialog may take focus");
      assert(tracker.isCurrent(epoch), "expected modal blur does not stale its continuation");
      assert(tracker.observeWindowFocus(), "focus restoration is paired with the active modal");
      return { selected: true as const };
    },
    async () => {
      lifecycleInvocations += 1;
      assert(tracker.observeWindowBlur(), "the native custody ceremony may take focus");
      assert(tracker.observeWindowFocus(), "custody ceremony focus is restored");
      return { locked: true as const };
    },
    async () => true,
  );
  assert(lifecycle.completed, `${operationName} continues after native selection`);
  assert(lifecycleInvocations === 1, `${operationName} is invoked exactly once`);
}

async function unrelatedFocusLossStillFailsClosed() {
  const tracker = new WalletPresentationEpoch();
  const epoch = tracker.capture();
  assert(!tracker.observeWindowBlur(), "blur without an owned native modal is not exempt");
  tracker.invalidate();
  let invoked = false;
  const result = await runWalletNativeModal(
    tracker,
    epoch,
    async () => {
      invoked = true;
    },
    async () => true,
  );
  assert(!result.completed, "stale continuation remains rejected");
  assert(!invoked, "stale work cannot invoke a later lifecycle command");
}

async function hiddenPageInvalidatesAnActiveNativeModal() {
  const tracker = new WalletPresentationEpoch();
  const epoch = tracker.capture();
  const result = await runWalletNativeModal(
    tracker,
    epoch,
    async () => {
      assert(tracker.observeWindowBlur(), "active modal owns its initial focus transition");
      tracker.invalidate();
      return true;
    },
    async () => false,
  );
  assert(!result.completed, "visibility or teardown invalidation wins over modal completion");
}

async function missingFocusReturnBlocksLifecycleContinuation() {
  const tracker = new WalletPresentationEpoch();
  const epoch = tracker.capture();
  let lifecycleInvocations = 0;
  const result = await runWalletNativeSelectionWorkflow(
    tracker,
    epoch,
    async () => {
      assert(tracker.observeWindowBlur(), "selection owns the native focus transition");
      return { selected: true as const };
    },
    async () => {
      lifecycleInvocations += 1;
    },
    async () => false,
  );
  assert(!result.completed, "missing focus restoration fails closed");
  assert(lifecycleInvocations === 0, "create or restore is not invoked without restored focus");
  assert(!tracker.isCurrent(epoch), "failed focus restoration invalidates the captured epoch");
}

export async function runWalletPresentationAsyncTests() {
  await nativeSelectionThenLifecycleContinues("create");
  await nativeSelectionThenLifecycleContinues("restore");
  await unrelatedFocusLossStillFailsClosed();
  await hiddenPageInvalidatesAnActiveNativeModal();
  await missingFocusReturnBlocksLifecycleContinuation();
}

assert(
  pendingReconciliationMessage({ state: "accepted_recording_pending", transaction_id: "1".repeat(64) })?.includes("Do not resubmit"),
  "accepted recording warnings prohibit resubmission",
);
