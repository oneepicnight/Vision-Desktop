export type WalletAccountSummary = {
  wallet_id: string;
  label: string | null;
  public_key: string;
  address: string;
  created_at_unix_ms: number;
  backup_verified: boolean | null;
};

export type WalletLifecycleStatus = {
  vault_exists: boolean;
  locked: boolean;
  account: WalletAccountSummary | null;
};

export type WalletRecoverySelection = {
  recovery_selection_handle: string;
};

export type WalletLockResult = {
  locked: true;
};

export type WalletCreateRequest = {
  wallet_id: string;
  label: string;
  recovery_destination_handle: string;
};

export type WalletRestoreRequest = {
  wallet_id: string;
  label: string;
  recovery_source_handle: string;
};

export type WalletTransferPreview = {
  preview_handle: string;
  sender: string;
  recipient: string;
  amount: string;
  amount_raw_units: string;
  charged_fee: string;
  charged_fee_raw_units: string;
  maximum_fee: string;
  fee_limit_raw_units: string;
  total_debit: string;
  total_debit_raw_units: string;
  balance: string;
  balance_raw_units: string;
  nonce: string;
  transaction_id: string;
  canonical_tip_height: string;
  canonical_tip_hash: string;
  core_contract: string;
  status_version: string;
  data_age_ms: string;
  expires_after_ms: string;
  warning: string;
};

export type WalletPublicObservation =
  | { state: "not_observed" }
  | { state: "pending" }
  | { state: "mined_observed"; confirmations: string }
  | { state: "mined_high_confidence"; confirmations: string };

export type WalletSubmissionOutcome =
  | {
      state: "accepted";
      transaction_id: string;
      observation: WalletPublicObservation;
    }
  | { state: "accepted_recording_pending"; transaction_id: string }
  | { state: "outcome_unknown"; transaction_id: string }
  | { state: "rejected"; transaction_id: string };

export type WalletPendingReconciliation =
  | { state: "accepted_recording_pending"; transaction_id: string }
  | { state: "outcome_unknown"; transaction_id: string };

export type WalletActivityRecord = {
  transaction_id: string;
  sender: string;
  recipient: string;
  amount_raw_units: string;
  nonce: string;
  tip_raw_units: string;
  fee_limit_raw_units: string;
  submitted_at_unix_ms: string;
  last_observed_at_unix_ms: string | null;
  observation: WalletPublicObservation;
};

export type WalletActivityResponse = {
  records: WalletActivityRecord[];
  history_complete: false;
  pending_reconciliation: WalletPendingReconciliation | null;
};

export type WalletReceiptRefreshResponse = {
  record: WalletActivityRecord;
  change: string;
};

export type WalletFixedError = {
  code: string;
};
