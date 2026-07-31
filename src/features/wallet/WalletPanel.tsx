import { Coins, Landmark, ShieldAlert } from "lucide-react";
import { Card } from "../../components/Card";
import { Metric } from "../../components/Metric";
import type { DesktopState } from "../../state/desktopState";
import { deriveWalletViewModel } from "./walletStatus";

type WalletPanelProps = {
  state: DesktopState;
};

export function WalletPanel({ state }: WalletPanelProps) {
  const viewModel = deriveWalletViewModel(state);

  return (
    <div className="grid wallet-grid">
      <Card title="Wallet Status" icon={<Landmark size={20} />}>
        <Metric label="Status" value={viewModel.overallStatus} />
        <Metric label="Core context" value={viewModel.coreContext} />
        <Metric label="Recovery state" value={viewModel.recoveryState} />
        <Metric label="Mock mode" value={viewModel.mockMode} />
        <Metric label="Last refresh" value={viewModel.lastRefresh} />
        <Metric label="Ownership / custody" value={viewModel.ownershipStatus} />
        <p className="note">{viewModel.summary}</p>
      </Card>

      <Card title="Address Sources" icon={<ShieldAlert size={20} />}>
        <Metric label="Configured reward address" value={viewModel.configuredAddress} />
        <Metric label="Configured source" value={viewModel.configuredAddressSource} />
        <Metric label="Live account address" value={viewModel.liveAddress} />
        <Metric label="Live address source" value={viewModel.liveAddressSource} />
        <Metric label="Lookup query" value={viewModel.lookupQuery} />
        <p className="empty-state">
          A configured reward address does not prove that Desktop controls, owns, or can spend from that address.
        </p>
      </Card>

      <Card title="Account Data" icon={<Coins size={20} />}>
        <Metric label="Balance availability" value={viewModel.balanceAvailability} />
        <Metric label="Balance" value={viewModel.balanceValue} />
        <Metric label="Denomination / precision" value={viewModel.denominationStatus} />
        <Metric label="Nonce" value={viewModel.nonceValue} />
        <Metric label="Account lookup" value={viewModel.lookupStatus} />
        <Metric label="Transaction history" value={viewModel.transactionHistoryStatus} />
        <p className="empty-state">
          This first Wallet page is read-only. It does not create wallets, store secrets, sign transactions, or submit transfers.
        </p>
      </Card>
    </div>
  );
}
