import {
  Binary,
  Blocks,
  CircleDotDashed,
  Coins,
  Database,
  FileSearch,
  Hash,
  Radar,
  Search,
  ShieldCheck,
  WalletCards,
  X,
} from "lucide-react";
import type { DesktopActions, DesktopState } from "../../state/desktopState";

type ExplorerPanelProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function ExplorerPanel({ state, actions }: ExplorerPanelProps) {
  const explorer = state.explorer;
  const status = state.snapshot?.status;
  const modeLabel = explorer.mode === "address" ? "Address" : "Transaction";
  const resultLabel = explorer.result == null
    ? "Awaiting query"
    : explorer.result.kind === "address"
      ? "Account record"
      : "Transaction record";

  return (
    <div className="explorer-command-center">
      <section className="explorer-hero" aria-labelledby="explorer-hero-title">
        <div className="explorer-hero-copy">
          <div className="explorer-hero-kicker">
            <Radar size={15} aria-hidden="true" />
            Vision Chain Intelligence
          </div>
          <h2 id="explorer-hero-title">Search the ledger</h2>
          <p>
            Inspect confirmed address or transaction data through the existing read-only Desktop boundary.
          </p>

          <div className="explorer-context-strip" aria-label="Explorer context">
            <div>
              <Blocks size={16} aria-hidden="true" />
              <span>Chain height</span>
              <strong>{status?.canonical_tip_height ?? "Unavailable"}</strong>
            </div>
            <div>
              <CircleDotDashed size={16} aria-hidden="true" />
              <span>Mempool</span>
              <strong>{status?.mempool_size ?? "Unavailable"}</strong>
            </div>
            <div>
              <Database size={16} aria-hidden="true" />
              <span>Core</span>
              <strong>{state.snapshot?.process_state ?? "Unknown"}</strong>
            </div>
            <div>
              <ShieldCheck size={16} aria-hidden="true" />
              <span>Source</span>
              <strong>{state.mockMode ? "Mock data" : "Core lookup"}</strong>
            </div>
          </div>
        </div>

        <div className="explorer-hero-visual" aria-hidden="true">
          <div className="explorer-scan-ring explorer-scan-ring-outer" />
          <div className="explorer-scan-ring explorer-scan-ring-inner" />
          <div className="explorer-scan-beam" />
          <div className="explorer-scan-core">
            <Search size={42} />
          </div>
          <span className="explorer-scan-node explorer-scan-node-one" />
          <span className="explorer-scan-node explorer-scan-node-two" />
          <span className="explorer-scan-node explorer-scan-node-three" />
        </div>
      </section>

      <section className="explorer-search-console" aria-labelledby="explorer-search-title">
        <div className="explorer-section-heading">
          <span className="explorer-section-icon">
            <FileSearch size={20} aria-hidden="true" />
          </span>
          <div>
            <h3 id="explorer-search-title">Ledger lookup</h3>
            <p>Choose one confirmed query type and inspect the returned record.</p>
          </div>
          <span className="explorer-readonly-badge">
            <ShieldCheck size={13} aria-hidden="true" />
            Read-only
          </span>
        </div>

        <div className="explorer-mode-switch" role="group" aria-label="Explorer lookup type">
          <button
            type="button"
            className={explorer.mode === "address" ? "active" : ""}
            onClick={() => actions.setExplorerMode("address")}
            disabled={explorer.loading}
            aria-pressed={explorer.mode === "address"}
          >
            <WalletCards size={17} aria-hidden="true" />
            Address
          </button>
          <button
            type="button"
            className={explorer.mode === "transaction" ? "active" : ""}
            onClick={() => actions.setExplorerMode("transaction")}
            disabled={explorer.loading}
            aria-pressed={explorer.mode === "transaction"}
          >
            <Binary size={17} aria-hidden="true" />
            Transaction
          </button>
        </div>

        <div className="explorer-query-row">
          <label>
            <span>{explorer.mode === "address" ? "Public address" : "Transaction ID"}</span>
            <div className="explorer-query-input">
              {explorer.mode === "address" ? (
                <WalletCards size={18} aria-hidden="true" />
              ) : (
                <Hash size={18} aria-hidden="true" />
              )}
              <input
                value={explorer.query}
                onChange={(event) => actions.setExplorerQuery(event.target.value)}
                placeholder={
                  explorer.mode === "address"
                    ? "Enter an address to inspect balance and nonce"
                    : "Enter a transaction ID to inspect the payload"
                }
                spellCheck={false}
              />
            </div>
          </label>
          <div className="explorer-query-actions">
            <button
              type="button"
              className="explorer-search-button"
              onClick={actions.searchExplorer}
              disabled={explorer.loading}
            >
              <Search size={18} aria-hidden="true" />
              {explorer.loading ? "Looking up" : `Lookup ${modeLabel}`}
            </button>
            <button
              type="button"
              className="explorer-clear-button"
              onClick={actions.clearExplorerResult}
              disabled={explorer.loading}
            >
              <X size={17} aria-hidden="true" />
              Clear
            </button>
          </div>
        </div>

        {state.mockMode ? (
          <p className="explorer-mode-note">
            Explorer is using the existing Desktop mock lookup path. No live Core response is represented.
          </p>
        ) : null}
      </section>

      <section className="explorer-result-console" aria-labelledby="explorer-result-title">
        <div className="explorer-section-heading explorer-result-heading">
          <span className="explorer-section-icon explorer-result-icon">
            <Database size={20} aria-hidden="true" />
          </span>
          <div>
            <h3 id="explorer-result-title">{resultLabel}</h3>
            <p>{modeLabel} lookup output from the current Desktop explorer state.</p>
          </div>
        </div>

        {explorer.error != null ? (
          <div className="explorer-error" role="alert">
            <CircleDotDashed size={18} aria-hidden="true" />
            <div>
              <strong>Lookup unavailable</strong>
              <p>{explorer.error}</p>
            </div>
          </div>
        ) : explorer.result == null ? (
          <div className="explorer-empty-result">
            <div className="explorer-empty-visual" aria-hidden="true">
              <Search size={31} />
            </div>
            <div>
              <strong>No record loaded</strong>
              <p>
                {explorer.mode === "address"
                  ? "Enter a public address to inspect its returned balance and nonce."
                  : "Enter a transaction ID to inspect the current Core-compatible payload."}
              </p>
            </div>
          </div>
        ) : explorer.result.kind === "address" ? (
          <div className="explorer-address-result">
            <div className="explorer-record-address">
              <span>Returned public address</span>
              <code title={explorer.result.address}>{explorer.result.address}</code>
            </div>
            <div className="explorer-record-grid">
              <div>
                <Coins size={19} aria-hidden="true" />
                <span>Balance</span>
                <strong>{explorer.result.balance}</strong>
                <small>Exact backend value; denomination metadata is not exposed</small>
              </div>
              <div>
                <Hash size={19} aria-hidden="true" />
                <span>Nonce</span>
                <strong>{explorer.result.nonce}</strong>
                <small>Exact value returned by the read-only lookup</small>
              </div>
            </div>
          </div>
        ) : (
          <div className="explorer-transaction-result">
            <div className="explorer-record-address">
              <span>Returned transaction ID</span>
              <code title={explorer.result.txid}>{explorer.result.txid}</code>
            </div>
            <div className="explorer-payload-heading">
              <Binary size={16} aria-hidden="true" />
              Core-compatible payload
            </div>
            <pre>{explorer.result.payload}</pre>
          </div>
        )}
      </section>
    </div>
  );
}
