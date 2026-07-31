import {
  Activity,
  Clock3,
  Cpu,
  Database,
  Gauge,
  Info,
  Pickaxe,
  ShieldCheck,
  WalletCards,
} from "lucide-react";
import { Metric } from "../../components/Metric";
import type { DesktopState } from "../../state/desktopState";
import { deriveMiningViewModel } from "./miningStatus";

type MiningPanelProps = {
  state: DesktopState;
};

function getStatusTone(headline: string) {
  if (headline === "Mining active") return "is-active";
  if (headline === "Mining enabled but idle" || headline === "Mock mining data") {
    return "is-standby";
  }
  if (headline === "Mining status unknown") return "is-unknown";
  return "is-blocked";
}

export function MiningPanel({ state }: MiningPanelProps) {
  const snapshot = state.snapshot;
  const viewModel = deriveMiningViewModel(state);
  const statusTone = getStatusTone(viewModel.headline);

  return (
    <div className="mining-command-center">
      <section className={`mining-hero ${statusTone}`} aria-labelledby="mining-hero-title">
        <div className="mining-hero-copy">
          <div className="mining-hero-kicker">
            <Pickaxe size={15} aria-hidden="true" />
            Vision Mining Operations
          </div>
          <div className="mining-hero-heading">
            <div>
              <span>Confirmed runtime observation</span>
              <h2 id="mining-hero-title">{viewModel.headline}</h2>
            </div>
            <span className="mining-readonly-badge">
              <ShieldCheck size={13} aria-hidden="true" />
              Read-only
            </span>
          </div>
          <p>{viewModel.detail}</p>

          <div className="mining-status-strip" aria-label="Current mining status">
            <div>
              <Activity size={16} aria-hidden="true" />
              <span>Activity</span>
              <strong>{viewModel.activity}</strong>
            </div>
            <div>
              <Gauge size={16} aria-hidden="true" />
              <span>Availability</span>
              <strong>{viewModel.availability}</strong>
            </div>
            <div>
              <Cpu size={16} aria-hidden="true" />
              <span>Core process</span>
              <strong>{viewModel.processReadiness}</strong>
            </div>
            <div>
              <Database size={16} aria-hidden="true" />
              <span>Height context</span>
              <strong>{viewModel.heightContext}</strong>
            </div>
          </div>
        </div>

        <div className="mining-reactor" aria-hidden="true">
          <div className="mining-reactor-ring mining-reactor-ring-outer" />
          <div className="mining-reactor-ring mining-reactor-ring-middle" />
          <div className="mining-reactor-ring mining-reactor-ring-inner" />
          <span className="mining-reactor-node mining-reactor-node-one" />
          <span className="mining-reactor-node mining-reactor-node-two" />
          <span className="mining-reactor-node mining-reactor-node-three" />
          <div className="mining-reactor-core">
            <Pickaxe size={42} />
          </div>
          <div className="mining-reactor-caption">
            <strong>{viewModel.runtimeEnabled}</strong>
            <span>runtime mining</span>
          </div>
        </div>
      </section>

      <div className="mining-content-grid">
        <section className="mining-status-panel" aria-labelledby="mining-runtime-title">
          <div className="mining-section-heading">
            <span className="mining-section-icon">
              <Activity size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="mining-runtime-title">Runtime observation</h3>
              <p>Values reported through the shared Desktop snapshot.</p>
            </div>
            <span className={`mining-status-pill ${statusTone}`}>{viewModel.headline}</span>
          </div>
          <div className="mining-metric-grid">
            <Metric label="Runtime enabled" value={viewModel.runtimeEnabled} />
            <Metric label="Activity" value={viewModel.activity} />
            <Metric label="Availability" value={viewModel.availability} />
            <Metric label="Process state" value={viewModel.processReadiness} />
            <Metric label="Height context" value={viewModel.heightContext} />
            <Metric label="Chain height" value={snapshot?.status?.canonical_tip_height ?? "Unavailable"} />
          </div>
        </section>

        <section className="mining-config-panel" aria-labelledby="mining-config-title">
          <div className="mining-section-heading">
            <span className="mining-section-icon mining-config-icon">
              <WalletCards size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="mining-config-title">Desktop configuration</h3>
              <p>Configured values do not prove live block production.</p>
            </div>
          </div>
          <Metric
            label="Mining configured"
            value={state.config.mining_enabled ? "Enabled" : "Disabled"}
          />
          <div className="mining-reward-address">
            <span>Configured reward address</span>
            <code title={viewModel.rewardAddress}>{viewModel.rewardAddress}</code>
            <small>Public configured value; custody and ownership are not proven.</small>
          </div>
        </section>

        <section className="mining-recovery-panel" aria-labelledby="mining-recovery-title">
          <div className="mining-section-heading">
            <span className="mining-section-icon mining-recovery-icon">
              <ShieldCheck size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="mining-recovery-title">Safety and recovery context</h3>
              <p>Existing Core observations only; Desktop does not invent recovery policy.</p>
            </div>
          </div>
          <Metric label="Recovery state" value={viewModel.recoveryState} />
          <Metric label="Paused reason" value={viewModel.pausedReason} />
          <Metric
            label="Core API"
            value={snapshot == null ? "Unavailable" : snapshot.api_error ?? "Connected"}
          />
          <Metric
            label="Mock mode"
            value={state.mockMode || snapshot?.mock_mode ? "Yes" : "No"}
          />
        </section>

        <section className="mining-boundary-panel" aria-labelledby="mining-boundary-title">
          <div className="mining-section-heading">
            <span className="mining-section-icon mining-boundary-icon">
              <Info size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="mining-boundary-title">Observation boundary</h3>
              <p>This command center intentionally contains no mining controls.</p>
            </div>
          </div>
          <div className="mining-freshness">
            <Clock3 size={17} aria-hidden="true" />
            <span>Desktop snapshot refreshed</span>
            <strong>{viewModel.lastUpdated}</strong>
          </div>
          <ul>
            <li>Enabled status does not prove that blocks are being produced.</li>
            <li>No start, stop, pause, resume, pool, farm, or performance controls are exposed.</li>
            <li>No reward, profitability, worker, or hashrate claims are derived by Desktop.</li>
          </ul>
        </section>
      </div>
    </div>
  );
}
