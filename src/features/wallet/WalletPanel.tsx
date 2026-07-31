import {
  BadgeCheck,
  Clock3,
  Coins,
  Fingerprint,
  Landmark,
  LockKeyhole,
  Radio,
  ShieldCheck,
  WalletCards,
} from "lucide-react";
import { Metric } from "../../components/Metric";
import type { DesktopState } from "../../state/desktopState";
import { deriveWalletViewModel } from "./walletStatus";

type WalletPanelProps = {
  state: DesktopState;
};

export function WalletPanel({ state }: WalletPanelProps) {
  const viewModel = deriveWalletViewModel(state);
  const hasLiveAddress = viewModel.liveAddress !== "Unavailable";
  const displayAddress = hasLiveAddress
    ? viewModel.liveAddress
    : viewModel.configuredAddress;
  const displayAddressSource = hasLiveAddress
    ? viewModel.liveAddressSource
    : viewModel.configuredAddressSource;

  return (
    <div className="wallet-command-center">
      <section className="wallet-hero" aria-labelledby="wallet-hero-title">
        <div className="wallet-hero-main">
          <div className="wallet-hero-kicker">
            <Landmark size={15} aria-hidden="true" />
            Vision Account Console
          </div>
          <div className="wallet-hero-heading">
            <div>
              <span className="wallet-label">Observed account balance</span>
              <h2 id="wallet-hero-title">{viewModel.balanceValue}</h2>
              <p>{viewModel.denominationStatus}</p>
            </div>
            <div className="wallet-hero-badges">
              <span className="wallet-badge wallet-badge-readonly">
                <LockKeyhole size={13} aria-hidden="true" />
                Read-only
              </span>
              <span className="wallet-badge">{viewModel.overallStatus}</span>
            </div>
          </div>

          <div className="wallet-address-block">
            <div className="wallet-address-heading">
              <span>{hasLiveAddress ? "Live account address" : "Configured reward address"}</span>
              <small>{displayAddressSource}</small>
            </div>
            <code title={displayAddress}>{displayAddress}</code>
          </div>

          <p className="wallet-hero-summary">{viewModel.summary}</p>
        </div>

        <div className="wallet-hero-visual" aria-hidden="true">
          <div className="wallet-vault-orbit wallet-vault-orbit-outer" />
          <div className="wallet-vault-orbit wallet-vault-orbit-inner" />
          <div className="wallet-vault-core">
            <ShieldCheck size={43} />
          </div>
          <span className="wallet-vault-node wallet-vault-node-one" />
          <span className="wallet-vault-node wallet-vault-node-two" />
          <div className="wallet-vault-caption">Public account data only</div>
        </div>
      </section>

      <section className="wallet-status-strip" aria-label="Wallet account context">
        <div className="wallet-status-card">
          <Coins size={17} aria-hidden="true" />
          <span>Balance</span>
          <strong>{viewModel.balanceAvailability}</strong>
        </div>
        <div className="wallet-status-card">
          <Fingerprint size={17} aria-hidden="true" />
          <span>Nonce</span>
          <strong>{viewModel.nonceValue}</strong>
        </div>
        <div className="wallet-status-card">
          <Radio size={17} aria-hidden="true" />
          <span>Core context</span>
          <strong>{viewModel.coreContext}</strong>
        </div>
        <div className="wallet-status-card">
          <BadgeCheck size={17} aria-hidden="true" />
          <span>Recovery</span>
          <strong>{viewModel.recoveryState}</strong>
        </div>
        <div className="wallet-status-card">
          <Clock3 size={17} aria-hidden="true" />
          <span>Last refresh</span>
          <strong>{viewModel.lastRefresh}</strong>
        </div>
      </section>

      <div className="wallet-detail-grid">
        <section className="wallet-console-card wallet-address-card">
          <div className="wallet-card-title">
            <span className="wallet-card-icon">
              <WalletCards size={19} aria-hidden="true" />
            </span>
            <div>
              <h3>Address provenance</h3>
              <p>Confirmed public identifiers and their source.</p>
            </div>
          </div>
          <Metric label="Configured reward address" value={viewModel.configuredAddress} />
          <Metric label="Configured source" value={viewModel.configuredAddressSource} />
          <Metric label="Live account address" value={viewModel.liveAddress} />
          <Metric label="Live address source" value={viewModel.liveAddressSource} />
          <Metric label="Lookup query" value={viewModel.lookupQuery} />
          <p className="wallet-card-note">
            A configured reward address does not prove that Desktop controls, owns, or can spend from that address.
          </p>
        </section>

        <section className="wallet-console-card wallet-account-card">
          <div className="wallet-card-title">
            <span className="wallet-card-icon">
              <Coins size={19} aria-hidden="true" />
            </span>
            <div>
              <h3>Account observation</h3>
              <p>Exact values from the existing read-only lookup path.</p>
            </div>
          </div>
          <Metric label="Balance" value={viewModel.balanceValue} />
          <Metric label="Denomination / precision" value={viewModel.denominationStatus} />
          <Metric label="Nonce" value={viewModel.nonceValue} />
          <Metric label="Account lookup" value={viewModel.lookupStatus} />
          <Metric label="Transaction history" value={viewModel.transactionHistoryStatus} />
          <p className="wallet-card-note">
            Balance text is preserved exactly as returned. Desktop performs no unit conversion or floating-point arithmetic.
          </p>
        </section>

        <section className="wallet-console-card wallet-security-card">
          <div className="wallet-card-title">
            <span className="wallet-card-icon wallet-card-icon-security">
              <ShieldCheck size={19} aria-hidden="true" />
            </span>
            <div>
              <h3>Desktop security boundary</h3>
              <p>What this account surface intentionally cannot do.</p>
            </div>
          </div>
          <ul className="wallet-security-list">
            <li><BadgeCheck size={15} />No private keys, seeds, or mnemonics</li>
            <li><BadgeCheck size={15} />No wallet creation, import, or export</li>
            <li><BadgeCheck size={15} />No signing or transaction submission</li>
            <li><BadgeCheck size={15} />No custody or ownership claim</li>
            <li><BadgeCheck size={15} />No automatic clipboard access</li>
          </ul>
          <div className="wallet-custody-status">
            <span>Custody status</span>
            <strong>{viewModel.ownershipStatus}</strong>
          </div>
        </section>
      </div>
    </div>
  );
}
