import type {
  WalletActivityResponse,
  WalletFixedError,
  WalletLifecycleStatus,
  WalletPendingReconciliation,
  WalletPublicObservation,
  WalletSubmissionOutcome,
} from "../../types/wallet";

const KNOWN_ERROR_MESSAGES = {
  invalid_request: "The wallet request was rejected.",
  invalid_window: "Wallet access is unavailable from this window.",
  wallet_activation_unavailable: "The secure wallet is not available in this build.",
  wallet_process_lock_unavailable: "Secure wallet process ownership is unavailable.",
  unsupported_windows_host: "Wallet custody is unavailable on this Windows host.",
  wallet_runtime_unavailable: "The secure wallet runtime is unavailable. Restart Vision Desktop.",
  wallet_unavailable: "The wallet is unavailable.",
  wallet_operation_in_progress: "Another wallet operation is already in progress.",
  operation_in_progress: "Another wallet operation is already in progress.",
  wallet_core_compatibility_unavailable: "The running Core is not compatible with this wallet.",
  wallet_core_unavailable: "Start the approved Vision Core before using transactions.",
  wallet_core_response_rejected: "Core returned a response that failed wallet validation.",
  wallet_core_recovering: "Core is recovering. Try again after it is ready.",
  wallet_account_unavailable: "The wallet account is unavailable.",
  insufficient_balance: "The wallet balance is not sufficient for this transfer and fee.",
  wallet_amount_arithmetic_rejected: "The transfer amount could not be represented safely.",
  wallet_preview_unavailable: "The transfer preview is no longer available.",
  wallet_confirmation_cancelled: "The native transfer confirmation was cancelled.",
  wallet_reconciliation_pending: "A prior transfer still requires reconciliation.",
  wallet_activity_unavailable: "Authenticated wallet activity is unavailable.",
  wallet_transaction_unknown: "This transaction is not in authenticated local wallet activity.",
  recovery_selection_cancelled: "Recovery selection was cancelled.",
  recovery_destination_invalid: "The recovery destination is invalid.",
  recovery_destination_exists: "The selected recovery destination already exists.",
  recovery_source_invalid: "The recovery source is invalid.",
  recovery_storage_unavailable: "The selected recovery file is unavailable.",
  path_authorization_invalid: "The recovery selection is no longer valid.",
  path_authorization_expired: "The recovery selection expired. Select it again.",
  invalid_label: "The wallet label was rejected.",
  password_policy: "The native password did not meet the wallet security policy.",
  invalid_password_or_damage: "The password was incorrect or the encrypted wallet data is damaged.",
  unlock_temporarily_blocked: "Unlock is temporarily blocked after repeated failures.",
  secure_random_unavailable: "Secure operating-system randomness is unavailable.",
  recovery_protection_unavailable: "Portable recovery protection is unavailable.",
  recovery_acknowledgement_cancelled: "Recovery acknowledgement was cancelled.",
  recovery_acknowledgement_unavailable: "The native recovery acknowledgement is unavailable.",
  recovery_backup_mismatch: "Recovery verification failed. No local wallet was published.",
  vault_protection_unavailable: "Local wallet protection is unavailable.",
  vault_storage_unavailable: "Encrypted local wallet storage is unavailable.",
  clock_unavailable: "The secure wallet clock is unavailable.",
  wallet_already_exists: "A wallet already exists on this device.",
  wallet_confirmation_authority_revoked: "Transfer confirmation authority was revoked.",
  wallet_confirmation_ui_unavailable: "The trusted native confirmation window is unavailable.",
  wallet_signing_unavailable: "Native transaction signing is unavailable.",
} as const;

export type WalletKnownErrorCode = keyof typeof KNOWN_ERROR_MESSAGES;

export class WalletPresentationEpoch {
  private current = 0;

  capture() {
    return this.current;
  }

  invalidate() {
    this.current += 1;
  }

  isCurrent(captured: number) {
    return captured === this.current;
  }
}

export function walletErrorCode(error: unknown): WalletKnownErrorCode | "wallet_unavailable" {
  let candidate: unknown;
  if (typeof error === "object" && error !== null && "code" in error) {
    candidate = (error as WalletFixedError).code;
  } else if (typeof error === "string" && error.length <= 64) {
    candidate = error;
  }
  return typeof candidate === "string" && candidate in KNOWN_ERROR_MESSAGES
    ? (candidate as WalletKnownErrorCode)
    : "wallet_unavailable";
}

export function walletErrorMessage(error: unknown) {
  return KNOWN_ERROR_MESSAGES[walletErrorCode(error)];
}

export function validWalletIdentity(walletId: string, label: string) {
  const idBytes = new TextEncoder().encode(walletId).length;
  const labelBytes = new TextEncoder().encode(label).length;
  return (
    idBytes >= 1 &&
    idBytes <= 64 &&
    /^[A-Za-z0-9_-]+$/.test(walletId) &&
    labelBytes >= 1 &&
    labelBytes <= 64 &&
    label.trim() === label &&
    !/\p{Cc}/u.test(label)
  );
}

export function validTransferDraft(recipient: string, amount: string) {
  return (
    /^[0-9a-f]{64}$/.test(recipient) &&
    /^(?:0|[1-9][0-9]*)(?:\.[0-9]{1,9})?$/.test(amount) &&
    !/^0(?:\.0{1,9})?$/.test(amount) &&
    amount.length <= 128
  );
}

export function observationLabel(observation: WalletPublicObservation) {
  switch (observation.state) {
    case "not_observed":
      return "Not observed";
    case "pending":
      return "Pending";
    case "mined_observed":
      return `Mined - ${observation.confirmations} confirmation(s)`;
    case "mined_high_confidence":
      return `Mined - high confidence - ${observation.confirmations} confirmation(s)`;
  }
}

export function pendingReconciliationMessage(pending: WalletPendingReconciliation | null) {
  if (pending == null) {
    return null;
  }
  return pending.state === "accepted_recording_pending"
    ? "Core accepted this transaction, but authenticated local recording is pending. Do not resubmit it."
    : "The prior submission outcome is unknown. Do not resubmit it while read-only reconciliation continues.";
}

export function submissionOutcomeMessage(outcome: WalletSubmissionOutcome) {
  switch (outcome.state) {
    case "accepted":
      return "Core accepted the transaction and authenticated local activity was recorded.";
    case "accepted_recording_pending":
      return "Core accepted the transaction, but authenticated local recording is pending. Do not resubmit it.";
    case "outcome_unknown":
      return "The submission outcome is unknown. Do not resubmit it; use read-only reconciliation.";
    case "rejected":
      return "Core definitively rejected the transaction under the approved compatibility contract.";
  }
}

export function spendingControlsEnabled(
  status: WalletLifecycleStatus | null,
  activity: WalletActivityResponse | null,
  busy: boolean,
) {
  return status?.locked === false && activity != null && activity.pending_reconciliation == null && !busy;
}
