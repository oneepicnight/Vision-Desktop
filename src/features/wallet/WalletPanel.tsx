import React from "react";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Clock3,
  FileKey2,
  Fingerprint,
  KeyRound,
  LockKeyhole,
  RefreshCw,
  Send,
  ShieldCheck,
  UnlockKeyhole,
  WalletCards,
} from "lucide-react";
import {
  walletCancelTransferPreview,
  walletConfirmAndSubmitTransfer,
  walletCreate,
  walletGetStatus,
  walletListActivity,
  walletLock,
  walletPrepareTransferPreview,
  walletRefreshTransactionObservation,
  walletRestore,
  walletSelectRecoveryDestination,
  walletSelectRecoverySource,
  walletUnlock,
} from "../../services/coreApi";
import type {
  WalletActivityResponse,
  WalletLifecycleStatus,
  WalletSubmissionOutcome,
  WalletTransferPreview,
} from "../../types/wallet";
import {
  observationLabel,
  pendingReconciliationMessage,
  spendingControlsEnabled,
  submissionOutcomeMessage,
  validTransferDraft,
  validWalletIdentity,
  walletErrorMessage,
} from "./walletPresentation";

type WalletOperation =
  | "status"
  | "create"
  | "restore"
  | "unlock"
  | "lock"
  | "preview"
  | "cancel"
  | "submit"
  | "activity"
  | "refresh";

function shortIdentifier(value: string) {
  return value.length > 20 ? `${value.slice(0, 10)}...${value.slice(-8)}` : value;
}

function publicTime(value: string | null) {
  if (value == null || !/^\d+$/.test(value)) return "Not observed";
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? new Date(parsed).toLocaleString() : "Recorded";
}

export function WalletPanel() {
  const [status, setStatus] = React.useState<WalletLifecycleStatus | null>(null);
  const [activity, setActivity] = React.useState<WalletActivityResponse | null>(null);
  const [preview, setPreview] = React.useState<WalletTransferPreview | null>(null);
  const [outcome, setOutcome] = React.useState<WalletSubmissionOutcome | null>(null);
  const [walletId, setWalletId] = React.useState("");
  const [walletLabel, setWalletLabel] = React.useState("");
  const [recipient, setRecipient] = React.useState("");
  const [amount, setAmount] = React.useState("");
  const [busy, setBusy] = React.useState<WalletOperation | null>(null);
  const [notice, setNotice] = React.useState("Secure wallet status has not been loaded.");
  const [error, setError] = React.useState<string | null>(null);
  const busyRef = React.useRef<WalletOperation | null>(null);

  const setOperation = React.useCallback((operation: WalletOperation | null) => {
    busyRef.current = operation;
    setBusy(operation);
  }, []);

  const clearPublicPresentation = React.useCallback(() => {
    setStatus(null);
    setActivity(null);
    setPreview(null);
    setOutcome(null);
    setWalletId("");
    setWalletLabel("");
    setRecipient("");
    setAmount("");
    setError(null);
    setNotice("Wallet presentation cleared. Refresh status to continue.");
  }, []);

  const loadActivity = React.useCallback(async () => {
    const next = await walletListActivity();
    setActivity(next);
    return next;
  }, []);

  const loadStatus = React.useCallback(async () => {
    if (busyRef.current != null) return;
    setOperation("status");
    setError(null);
    try {
      const next = await walletGetStatus();
      setStatus(next);
      setPreview(null);
      setOutcome(null);
      if (next.locked || !next.vault_exists) {
        setActivity(null);
      } else {
        await loadActivity();
      }
      setNotice(next.vault_exists ? (next.locked ? "Wallet is locked." : "Wallet is unlocked.") : "No local wallet exists yet.");
    } catch (reason) {
      clearPublicPresentation();
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  }, [clearPublicPresentation, loadActivity, setOperation]);

  React.useEffect(() => {
    void loadStatus();
    const clear = () => clearPublicPresentation();
    const onVisibility = () => {
      if (document.visibilityState === "hidden") clear();
      else void loadStatus();
    };
    const onFocus = () => {
      clear();
      if (busyRef.current == null) void loadStatus();
    };
    window.addEventListener("blur", clear);
    window.addEventListener("focus", onFocus);
    window.addEventListener("pagehide", clear);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("blur", clear);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("pagehide", clear);
      document.removeEventListener("visibilitychange", onVisibility);
      clear();
    };
  }, [clearPublicPresentation, loadStatus]);

  const runCreate = async () => {
    if (busy != null || status == null || status.vault_exists || !validWalletIdentity(walletId, walletLabel)) return;
    setOperation("create");
    setError(null);
    setPreview(null);
    try {
      const selection = await walletSelectRecoveryDestination();
      const next = await walletCreate({
        wallet_id: walletId,
        label: walletLabel,
        recovery_destination_handle: selection.recovery_selection_handle,
      });
      setStatus(next);
      setActivity(null);
      setWalletId("");
      setWalletLabel("");
      setNotice("Wallet created and portable recovery verified. The new wallet remains locked.");
    } catch (reason) {
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runRestore = async () => {
    if (busy != null || status == null || status.vault_exists || !validWalletIdentity(walletId, walletLabel)) return;
    setOperation("restore");
    setError(null);
    setPreview(null);
    try {
      const selection = await walletSelectRecoverySource();
      const next = await walletRestore({
        wallet_id: walletId,
        label: walletLabel,
        recovery_source_handle: selection.recovery_selection_handle,
      });
      setStatus(next);
      setActivity(null);
      setWalletId("");
      setWalletLabel("");
      setNotice("Wallet restored from the selected recovery artifact and remains locked.");
    } catch (reason) {
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runUnlock = async () => {
    if (busy != null || status?.vault_exists !== true || !status.locked) return;
    setOperation("unlock");
    setError(null);
    try {
      const next = await walletUnlock();
      setStatus(next);
      setPreview(null);
      setOutcome(null);
      await loadActivity();
      setNotice("Wallet unlocked. Authenticated activity and reconciliation state were loaded.");
    } catch (reason) {
      setActivity(null);
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runLock = async () => {
    if (busy != null || status?.locked !== false) return;
    setOperation("lock");
    setError(null);
    try {
      await walletLock();
      clearPublicPresentation();
      const next = await walletGetStatus();
      setStatus(next);
      setNotice("Wallet locked and public transaction presentation cleared.");
    } catch (reason) {
      clearPublicPresentation();
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runPrepare = async () => {
    if (!spendingControlsEnabled(status, activity, busy != null) || !validTransferDraft(recipient, amount)) return;
    setOperation("preview");
    setError(null);
    setOutcome(null);
    try {
      const next = await walletPrepareTransferPreview(recipient, amount);
      setPreview(next);
      setNotice("Transfer preview prepared from fresh authenticated Core data. Review every field.");
    } catch (reason) {
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runCancel = async () => {
    if (busy != null || preview == null) return;
    const handle = preview.preview_handle;
    setOperation("cancel");
    setError(null);
    setPreview(null);
    try {
      await walletCancelTransferPreview(handle);
      setNotice("Transfer preview cancelled and its handle consumed.");
    } catch (reason) {
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runSubmit = async () => {
    if (!spendingControlsEnabled(status, activity, busy != null) || preview == null) return;
    const handle = preview.preview_handle;
    setOperation("submit");
    setError(null);
    setPreview(null);
    // Submission may create durable ambiguity before any response escapes. Clear the previously
    // reconciled view first so a failed follow-up discovery can never re-enable spending.
    setActivity(null);
    try {
      const next = await walletConfirmAndSubmitTransfer(handle);
      setOutcome(next);
      setNotice(submissionOutcomeMessage(next));
      await loadActivity();
      setRecipient("");
      setAmount("");
    } catch (reason) {
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runRefreshActivity = async () => {
    if (busy != null || status?.locked !== false) return;
    setOperation("activity");
    setError(null);
    try {
      await loadActivity();
      setNotice("Authenticated activity and reconciliation state refreshed.");
    } catch (reason) {
      setActivity(null);
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const runRefreshRecord = async (transactionId: string) => {
    if (busy != null || status?.locked !== false) return;
    setOperation("refresh");
    setError(null);
    try {
      await walletRefreshTransactionObservation(transactionId);
      await loadActivity();
      setNotice("Transaction observation refreshed through the authenticated read-only path.");
    } catch (reason) {
      setError(walletErrorMessage(reason));
    } finally {
      setOperation(null);
    }
  };

  const pendingMessage = pendingReconciliationMessage(activity?.pending_reconciliation ?? null);
  const maySpend = spendingControlsEnabled(status, activity, busy != null);
  const account = status?.account ?? null;

  return (
    <div className="wallet-command-center wallet-custody-console">
      <section className="wallet-hero" aria-labelledby="wallet-hero-title">
        <div className="wallet-hero-main">
          <div className="wallet-hero-kicker"><ShieldCheck size={15} />Rust-owned custody</div>
          <div className="wallet-hero-heading">
            <div>
              <span className="wallet-label">Secure wallet state</span>
              <h2 id="wallet-hero-title">{status == null ? "Unavailable" : status.locked ? "Locked" : "Unlocked"}</h2>
              <p>{status?.vault_exists ? "Encrypted local vault detected" : "Create or restore a wallet to begin"}</p>
            </div>
            <div className="wallet-hero-badges">
              <span className="wallet-badge"><KeyRound size={13} />Native secrets only</span>
              <span className="wallet-badge wallet-badge-live">12-command atomic boundary</span>
            </div>
          </div>
          <div className="wallet-address-block">
            <div className="wallet-address-heading"><span>Custodied public address</span><small>{account == null ? "Available after create, restore, or unlock" : "Derived and verified in Rust"}</small></div>
            <code title={account?.address}>{account?.address ?? "No verified custodial address loaded"}</code>
          </div>
          <p className="wallet-hero-summary">{notice}</p>
          {error != null ? <div className="wallet-alert wallet-alert-error" role="alert"><AlertTriangle size={16} />{error}</div> : null}
          {pendingMessage != null ? <div className="wallet-alert wallet-alert-warning" role="alert"><Clock3 size={16} />{pendingMessage}</div> : null}
        </div>
        <div className="wallet-hero-visual" aria-hidden="true">
          <div className="wallet-vault-orbit wallet-vault-orbit-outer" />
          <div className="wallet-vault-orbit wallet-vault-orbit-inner" />
          <div className="wallet-vault-core">{status?.locked === false ? <UnlockKeyhole size={43} /> : <LockKeyhole size={43} />}</div>
          <span className="wallet-vault-node wallet-vault-node-one" />
          <span className="wallet-vault-node wallet-vault-node-two" />
          <div className="wallet-vault-caption">Secrets never enter React</div>
        </div>
      </section>

      <section className="wallet-status-strip" aria-label="Wallet security state">
        <div className="wallet-status-card"><WalletCards size={17} /><span>Vault</span><strong>{status == null ? "Unknown" : status.vault_exists ? "Present" : "Not found"}</strong></div>
        <div className="wallet-status-card"><LockKeyhole size={17} /><span>Session</span><strong>{status == null ? "Unknown" : status.locked ? "Locked" : "Unlocked"}</strong></div>
        <div className="wallet-status-card"><Fingerprint size={17} /><span>Wallet ID</span><strong>{account?.wallet_id ?? "Unavailable"}</strong></div>
        <div className="wallet-status-card"><FileKey2 size={17} /><span>Recovery</span><strong>{account?.backup_verified === true ? "Verified" : "Unknown"}</strong></div>
        <div className="wallet-status-card"><Activity size={17} /><span>Reconciliation</span><strong>{activity == null ? "Not loaded" : activity.pending_reconciliation == null ? "Clear" : "Pending"}</strong></div>
      </section>

      <div className="wallet-action-bar">
        <button type="button" className="secondary" onClick={() => void loadStatus()} disabled={busy != null}><RefreshCw size={15} />Refresh status</button>
        {status?.vault_exists ? (
          status.locked ? <button type="button" onClick={() => void runUnlock()} disabled={busy != null}><UnlockKeyhole size={15} />Unlock in native window</button>
          : <button type="button" className="danger" onClick={() => void runLock()} disabled={busy != null}><LockKeyhole size={15} />Lock wallet</button>
        ) : null}
        {busy != null ? <span className="wallet-busy" aria-live="polite">Secure operation: {busy}</span> : null}
      </div>

      {status != null && !status.vault_exists ? (
        <section className="wallet-console-card wallet-onboarding-card">
          <div className="wallet-card-title"><span className="wallet-card-icon"><FileKey2 size={19} /></span><div><h3>Create or restore</h3><p>Only a public identifier and label enter React. Passwords and recovery credentials use native Rust controls.</p></div></div>
          <div className="wallet-public-form">
            <label>Wallet identifier<input value={walletId} maxLength={64} autoComplete="off" spellCheck={false} onChange={(event) => setWalletId(event.target.value)} placeholder="operator_wallet" /></label>
            <label>Wallet label<input value={walletLabel} maxLength={64} autoComplete="off" spellCheck={false} onChange={(event) => setWalletLabel(event.target.value)} placeholder="Primary Wallet" /></label>
          </div>
          <div className="wallet-form-actions">
            <button type="button" onClick={() => void runCreate()} disabled={busy != null || !validWalletIdentity(walletId, walletLabel)}>Create with new recovery file</button>
            <button type="button" className="secondary" onClick={() => void runRestore()} disabled={busy != null || !validWalletIdentity(walletId, walletLabel)}>Restore from recovery file</button>
          </div>
          <p className="wallet-card-note">Creation requires a verified portable recovery file before the encrypted local vault is published.</p>
        </section>
      ) : null}

      {status?.locked === false ? (
        <div className="wallet-detail-grid wallet-live-grid">
          <section className="wallet-console-card wallet-send-card">
            <div className="wallet-card-title"><span className="wallet-card-icon"><Send size={19} /></span><div><h3>Send Vision</h3><p>Prepare from fresh Core balance, nonce, fee, and canonical-tip data.</p></div></div>
            <div className="wallet-public-form wallet-send-form">
              <label>Recipient address<input value={recipient} maxLength={64} autoComplete="off" spellCheck={false} onChange={(event) => { setRecipient(event.target.value); setPreview(null); }} placeholder="64 lowercase hexadecimal characters" /></label>
              <label>Amount<input value={amount} maxLength={128} inputMode="decimal" autoComplete="off" onChange={(event) => { setAmount(event.target.value); setPreview(null); }} placeholder="0.000000001" /></label>
            </div>
            <button type="button" onClick={() => void runPrepare()} disabled={!maySpend || !validTransferDraft(recipient, amount)}>Prepare exact transfer</button>
            {activity == null ? <p className="wallet-card-note wallet-warning-copy">Spending stays disabled until authenticated activity and reconciliation state load successfully.</p> : null}
          </section>

          <section className="wallet-console-card wallet-preview-card">
            <div className="wallet-card-title"><span className="wallet-card-icon"><ShieldCheck size={19} /></span><div><h3>Exact preview</h3><p>The final approval appears in a separate trusted native window.</p></div></div>
            {preview == null ? <p className="wallet-empty-state">No active transfer preview.</p> : <>
              <dl className="wallet-preview-values">
                <div><dt>Sender</dt><dd title={preview.sender}>{shortIdentifier(preview.sender)}</dd></div>
                <div><dt>Recipient</dt><dd title={preview.recipient}>{shortIdentifier(preview.recipient)}</dd></div>
                <div><dt>Amount</dt><dd>{preview.amount}</dd></div>
                <div><dt>Charged fee</dt><dd>{preview.charged_fee}</dd></div>
                <div><dt>Maximum fee</dt><dd>{preview.maximum_fee}</dd></div>
                <div><dt>Total debit</dt><dd>{preview.total_debit}</dd></div>
                <div><dt>Balance</dt><dd>{preview.balance}</dd></div>
                <div><dt>Nonce</dt><dd>{preview.nonce}</dd></div>
                <div><dt>Transaction ID</dt><dd title={preview.transaction_id}>{shortIdentifier(preview.transaction_id)}</dd></div>
                <div><dt>Data age</dt><dd>{preview.data_age_ms} ms</dd></div>
              </dl>
              <p className="wallet-card-note wallet-warning-copy">{preview.warning}</p>
              <div className="wallet-form-actions">
                <button type="button" onClick={() => void runSubmit()} disabled={!maySpend}>Open native confirmation</button>
                <button type="button" className="secondary" onClick={() => void runCancel()} disabled={busy != null}>Cancel preview</button>
              </div>
            </>}
            {outcome != null ? <div className={`wallet-outcome wallet-outcome-${outcome.state}`}><CheckCircle2 size={16} /><div><strong>{outcome.state.replaceAll("_", " ")}</strong><code>{outcome.transaction_id}</code><p>{submissionOutcomeMessage(outcome)}</p></div></div> : null}
          </section>

          <section className="wallet-console-card wallet-activity-card">
            <div className="wallet-card-title wallet-card-title-actions"><span className="wallet-card-icon"><Activity size={19} /></span><div><h3>Authenticated local activity</h3><p>Newest 100 local records. This is not complete chain history.</p></div><button type="button" className="secondary compact" onClick={() => void runRefreshActivity()} disabled={busy != null}><RefreshCw size={14} />Refresh</button></div>
            {activity?.records.length ? <div className="wallet-activity-list">{activity.records.map((record) => <article key={record.transaction_id} className="wallet-activity-row">
              <div><strong>{observationLabel(record.observation)}</strong><code title={record.transaction_id}>{shortIdentifier(record.transaction_id)}</code></div>
              <div><span>To {shortIdentifier(record.recipient)}</span><span>Raw amount {record.amount_raw_units}</span><span>{publicTime(record.last_observed_at_unix_ms ?? record.submitted_at_unix_ms)}</span></div>
              <button type="button" className="secondary compact" onClick={() => void runRefreshRecord(record.transaction_id)} disabled={busy != null}>Refresh observation</button>
            </article>)}</div> : <p className="wallet-empty-state">No authenticated local activity records.</p>}
          </section>
        </div>
      ) : null}
    </div>
  );
}
