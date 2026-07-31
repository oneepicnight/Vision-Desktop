import { Database, Search } from "lucide-react";
import { Card } from "../../components/Card";
import type { DesktopActions, DesktopState } from "../../state/desktopState";

type ExplorerPanelProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function ExplorerPanel({ state, actions }: ExplorerPanelProps) {
  const explorer = state.explorer;

  return (
    <div className="grid explorer-grid">
      <Card title="Explorer Query" icon={<Search size={20} />}>
        <div className="explorer-form">
          <label>
            Lookup type
            <select
              value={explorer.mode}
              onChange={(event) => actions.setExplorerMode(event.target.value as typeof explorer.mode)}
            >
              <option value="address">Address</option>
              <option value="transaction">Transaction</option>
            </select>
          </label>
          <label className="wide">
            {explorer.mode === "address" ? "Address" : "Transaction ID"}
            <input
              value={explorer.query}
              onChange={(event) => actions.setExplorerQuery(event.target.value)}
              placeholder={
                explorer.mode === "address"
                  ? "Enter an address to inspect balance and nonce"
                  : "Enter a transaction ID to inspect the payload"
              }
            />
          </label>
          <div className="actions">
            <button onClick={actions.searchExplorer} disabled={explorer.loading}>
              <Search size={18} />
              {explorer.loading ? "Looking up" : "Lookup"}
            </button>
            <button onClick={actions.clearExplorerResult} disabled={explorer.loading}>
              Clear
            </button>
          </div>
          {state.mockMode ? (
            <p className="note">
              Explorer is showing mock data. Switch mock mode off after private Core launch is available.
            </p>
          ) : null}
        </div>
      </Card>

      <Card title="Explorer Result" icon={<Database size={20} />}>
        {explorer.result == null ? (
          <p className="empty-state">
            {explorer.mode === "address"
              ? "Look up an address to inspect its current balance and nonce."
              : "Look up a transaction ID to inspect the current Core API payload."}
          </p>
        ) : explorer.result.kind === "address" ? (
          <div>
            <div className="metric">
              <span>Address</span>
              <strong>{explorer.result.address}</strong>
            </div>
            <div className="metric">
              <span>Balance</span>
              <strong>{explorer.result.balance}</strong>
            </div>
            <div className="metric">
              <span>Nonce</span>
              <strong>{explorer.result.nonce}</strong>
            </div>
          </div>
        ) : (
          <div className="json-block">
            <div className="metric">
              <span>Transaction ID</span>
              <strong>{explorer.result.txid}</strong>
            </div>
            <pre>{explorer.result.payload}</pre>
          </div>
        )}
      </Card>
    </div>
  );
}
