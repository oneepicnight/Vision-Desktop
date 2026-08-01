import {
  Activity,
  Ban,
  BarChart3,
  Clock3,
  DollarSign,
  LandPlot,
  LockKeyhole,
  Radio,
  ShieldCheck,
  ShoppingBag,
  Store,
} from "lucide-react";
import type { DesktopState } from "../../state/desktopState";
import { deriveMarketplaceViewModel } from "./marketplaceStatus";

type MarketplacePanelProps = {
  state: DesktopState;
};

const legacyModules = [
  {
    title: "Exchange market data",
    description: "Order books, tickers, trades, and account orders require an approved typed service.",
    icon: BarChart3,
  },
  {
    title: "Land marketplace",
    description: "Listings, settlement state, and ownership claims are not exposed by Desktop.",
    icon: LandPlot,
  },
  {
    title: "Cash order operations",
    description: "Checkout, payment, webhook, mint, and replay flows are deliberately excluded.",
    icon: DollarSign,
  },
] as const;

export function MarketplacePanel({ state }: MarketplacePanelProps) {
  const viewModel = deriveMarketplaceViewModel(state);

  return (
    <div className="marketplace-command-center">
      <section className="marketplace-hero" aria-labelledby="marketplace-hero-title">
        <div className="marketplace-hero-copy">
          <div className="marketplace-hero-kicker">
            <Store size={15} aria-hidden="true" />
            Vision Marketplace Observatory
          </div>
          <div className="marketplace-hero-heading">
            <div>
              <span>Read-only integration boundary</span>
              <h2 id="marketplace-hero-title">{viewModel.headline}</h2>
            </div>
            <span className="marketplace-readonly-badge">
              <LockKeyhole size={13} aria-hidden="true" />
              No transactions
            </span>
          </div>
          <p>{viewModel.summary}</p>
        </div>

        <div className="marketplace-hero-visual" aria-hidden="true">
          <div className="marketplace-market-ring marketplace-market-ring-outer" />
          <div className="marketplace-market-ring marketplace-market-ring-inner" />
          <div className="marketplace-market-core">
            <ShoppingBag size={40} />
          </div>
          <span className="marketplace-market-node marketplace-market-node-one" />
          <span className="marketplace-market-node marketplace-market-node-two" />
          <span className="marketplace-market-node marketplace-market-node-three" />
        </div>
      </section>

      <section className="marketplace-status-strip" aria-label="Marketplace integration context">
        <div className="marketplace-status-card">
          <Radio size={17} aria-hidden="true" />
          <span>Core context</span>
          <strong>{viewModel.coreContext}</strong>
        </div>
        <div className="marketplace-status-card">
          <ShieldCheck size={17} aria-hidden="true" />
          <span>Recovery</span>
          <strong>{viewModel.recoveryState}</strong>
        </div>
        <div className="marketplace-status-card">
          <Activity size={17} aria-hidden="true" />
          <span>Market feed</span>
          <strong>{viewModel.marketDataStatus}</strong>
        </div>
        <div className="marketplace-status-card">
          <Ban size={17} aria-hidden="true" />
          <span>Actions</span>
          <strong>Unavailable</strong>
        </div>
        <div className="marketplace-status-card">
          <Clock3 size={17} aria-hidden="true" />
          <span>Desktop refresh</span>
          <strong>{viewModel.lastRefresh}</strong>
        </div>
      </section>

      <div className="marketplace-content-grid">
        <section className="marketplace-terminal-card" aria-labelledby="marketplace-terminal-title">
          <div className="marketplace-card-heading">
            <span className="marketplace-card-icon">
              <BarChart3 size={19} aria-hidden="true" />
            </span>
            <div>
              <h3 id="marketplace-terminal-title">Market terminal</h3>
              <p>Legacy exchange hierarchy without invented market facts.</p>
            </div>
            <span className="marketplace-offline-pill">Feed disconnected</span>
          </div>

          <div className="marketplace-terminal-grid">
            <div className="marketplace-book-shell">
              <div className="marketplace-book-header">
                <span>Price</span>
                <span>Size</span>
                <span>Total</span>
              </div>
              <div className="marketplace-empty-book marketplace-empty-book-ask">
                <span>No confirmed asks</span>
              </div>
              <div className="marketplace-price-gap">No confirmed ticker</div>
              <div className="marketplace-empty-book marketplace-empty-book-bid">
                <span>No confirmed bids</span>
              </div>
            </div>

            <div className="marketplace-activity-shell">
              <div>
                <span>Recent market activity</span>
                <strong>Unavailable</strong>
              </div>
              <p>
                Desktop does not poll the legacy local endpoints or present fallback prices, volume, balances, or trades.
              </p>
            </div>
          </div>
        </section>

        <section className="marketplace-module-card" aria-labelledby="marketplace-modules-title">
          <div className="marketplace-card-heading">
            <span className="marketplace-card-icon">
              <Store size={19} aria-hidden="true" />
            </span>
            <div>
              <h3 id="marketplace-modules-title">Legacy feature map</h3>
              <p>Presentation targets awaiting approved APIs.</p>
            </div>
          </div>

          <div className="marketplace-module-list">
            {legacyModules.map(({ title, description, icon: Icon }) => (
              <div className="marketplace-module-row" key={title}>
                <span className="marketplace-module-icon">
                  <Icon size={17} aria-hidden="true" />
                </span>
                <div>
                  <strong>{title}</strong>
                  <p>{description}</p>
                </div>
                <span>Not connected</span>
              </div>
            ))}
          </div>
        </section>

        <section className="marketplace-boundary-card" aria-labelledby="marketplace-boundary-title">
          <div className="marketplace-card-heading">
            <span className="marketplace-card-icon marketplace-card-icon-safe">
              <ShieldCheck size={19} aria-hidden="true" />
            </span>
            <div>
              <h3 id="marketplace-boundary-title">Desktop safety boundary</h3>
              <p>Financial behavior remains outside this slice.</p>
            </div>
          </div>

          <ul className="marketplace-boundary-list">
            <li><ShieldCheck size={15} />No direct browser or hard-coded localhost requests</li>
            <li><ShieldCheck size={15} />No duplicate market polling loop or WebSocket</li>
            <li><ShieldCheck size={15} />No floating-point amount or price calculations</li>
            <li><ShieldCheck size={15} />No buy, sell, checkout, order, mint, or replay actions</li>
            <li><ShieldCheck size={15} />No wallet keys, custody, signing, or ownership claims</li>
          </ul>

          <div className="marketplace-integration-note">
            <span>Required next boundary</span>
            <strong>{viewModel.integrationStatus}</strong>
            <small>{viewModel.actionStatus}</small>
          </div>
        </section>
      </div>
    </div>
  );
}
