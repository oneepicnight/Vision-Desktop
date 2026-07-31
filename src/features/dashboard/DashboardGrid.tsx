import {
  Activity,
  Boxes,
  CircleDotDashed,
  Cpu,
  Database,
  FileArchive,
  FolderOpen,
  Gauge,
  HardDrive,
  MemoryStick,
  Network,
  Pickaxe,
  RadioTower,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react";
import { Metric } from "../../components/Metric";
import type { DesktopActions, DesktopState } from "../../state/desktopState";
import { bytes, shortHash } from "../../utils/format";
import { NetworkOverview } from "./NetworkOverview";

type DashboardGridProps = {
  state: DesktopState;
  actions: DesktopActions;
};

function statusTone(value: string) {
  if (value === "running" || value === "normal") return "is-healthy";
  if (value === "stopped" || value === "Unknown") return "is-neutral";
  return "is-warning";
}

export function DashboardGrid({ state, actions }: DashboardGridProps) {
  const status = state.snapshot?.status;
  const mining = state.snapshot?.mining ?? status?.mining;
  const recovery = status?.recovery;
  const processState = state.snapshot?.process_state ?? state.process?.state ?? "Unknown";
  const recoveryState = recovery?.state ?? "Unknown";
  const miningAvailable = state.snapshot?.mining?.enabled ?? status?.mining.available ?? false;
  const dataSource = state.mockMode || state.snapshot?.mock_mode ? "Mock snapshot" : "Core snapshot";

  return (
    <div className="dashboard-layout">
      <NetworkOverview state={state} actions={actions} />

      <section className="dashboard-operations" aria-labelledby="dashboard-operations-title">
        <div className="dashboard-operations-heading">
          <span className="dashboard-operations-icon">
            <Activity size={20} aria-hidden="true" />
          </span>
          <div>
            <span>Confirmed operator telemetry</span>
            <h2 id="dashboard-operations-title">Node operations grid</h2>
            <p>Current Desktop snapshot and process observations without inferred protocol behavior.</p>
          </div>
          <div className="dashboard-operations-source">
            <ShieldCheck size={14} aria-hidden="true" />
            <span>{dataSource}</span>
          </div>
        </div>

        <div className="dashboard-operations-grid">
          <section className="dashboard-operation-card dashboard-core-card" aria-labelledby="dashboard-core-title">
            <div className="dashboard-card-heading">
              <span className="dashboard-card-icon">
                <Activity size={20} aria-hidden="true" />
              </span>
              <div>
                <span>Runtime supervisor</span>
                <h3 id="dashboard-core-title">Core process</h3>
              </div>
              <span className={`dashboard-status-badge ${statusTone(processState)}`}>{processState}</span>
            </div>
            <div className="dashboard-primary-stat">
              <span>Observed process</span>
              <strong>{processState}</strong>
              <small>Command completion and observed state remain separate facts.</small>
            </div>
            <Metric label="PID" value={state.process?.pid ?? "Not running"} />
            <Metric label="Private API port" value={state.process?.api_port ?? "Private loopback"} />
            <Metric label="P2P port" value={state.process?.p2p_port ?? "Not listening"} />
          </section>

          <section className="dashboard-operation-card dashboard-chain-card" aria-labelledby="dashboard-chain-title">
            <div className="dashboard-card-heading">
              <span className="dashboard-card-icon is-purple">
                <Database size={20} aria-hidden="true" />
              </span>
              <div>
                <span>Canonical observation</span>
                <h3 id="dashboard-chain-title">Chain</h3>
              </div>
            </div>
            <div className="dashboard-primary-stat is-purple">
              <span>Canonical height</span>
              <strong>{status?.canonical_tip_height ?? "Unavailable"}</strong>
              <small>Height reported by the current Desktop snapshot.</small>
            </div>
            <Metric
              label="Tip hash"
              value={<span title={status?.canonical_tip_hash ?? undefined}>{shortHash(status?.canonical_tip_hash)}</span>}
            />
            <Metric label="Local work" value={recovery?.local_work ?? "Unavailable"} />
            <Metric
              label="State root"
              value={<span title={status?.cached_state_root ?? undefined}>{shortHash(status?.cached_state_root)}</span>}
            />
          </section>

          <section className="dashboard-operation-card dashboard-network-card" aria-labelledby="dashboard-network-title">
            <div className="dashboard-card-heading">
              <span className="dashboard-card-icon is-cyan">
                <Network size={20} aria-hidden="true" />
              </span>
              <div>
                <span>Connection snapshot</span>
                <h3 id="dashboard-network-title">Network</h3>
              </div>
            </div>
            <div className="dashboard-primary-stat is-cyan">
              <span>Reported peers</span>
              <strong>{status?.peer_count ?? 0}</strong>
              <small>Count only; Desktop does not infer trust or network resilience.</small>
            </div>
            <div className="dashboard-inline-stats">
              <div><span>Durable</span><strong>{status?.durable_peer_count ?? 0}</strong></div>
              <div><span>Inbound</span><strong>{status?.active_inbound_sessions ?? 0}</strong></div>
              <div><span>Outbound</span><strong>{status?.active_outbound_sessions ?? 0}</strong></div>
              <div><span>Transient</span><strong>{status?.transient_peer_count ?? 0}</strong></div>
            </div>
          </section>

          <section className="dashboard-operation-card dashboard-mining-card" aria-labelledby="dashboard-mining-title">
            <div className="dashboard-card-heading">
              <span className="dashboard-card-icon is-gold">
                <Pickaxe size={20} aria-hidden="true" />
              </span>
              <div>
                <span>Runtime signals</span>
                <h3 id="dashboard-mining-title">Mining and recovery</h3>
              </div>
              <span className={`dashboard-status-badge ${statusTone(recoveryState)}`}>{recoveryState}</span>
            </div>
            <div className="dashboard-signal-grid">
              <div>
                <Gauge size={17} aria-hidden="true" />
                <span>Available</span>
                <strong>{String(miningAvailable)}</strong>
              </div>
              <div>
                <RadioTower size={17} aria-hidden="true" />
                <span>Active</span>
                <strong>{String(mining?.active ?? false)}</strong>
              </div>
            </div>
            <Metric label="Paused reason" value={mining?.paused_reason ?? "None"} />
            <Metric label="Recovery state" value={recoveryState} />
            <p className="dashboard-card-boundary">Enabled or available status does not prove live block production.</p>
          </section>

          <section className="dashboard-operation-card dashboard-resources-card" aria-labelledby="dashboard-resources-title">
            <div className="dashboard-card-heading">
              <span className="dashboard-card-icon is-violet">
                <Cpu size={20} aria-hidden="true" />
              </span>
              <div>
                <span>Desktop observations</span>
                <h3 id="dashboard-resources-title">Mempool and resources</h3>
              </div>
            </div>
            <div className="dashboard-resource-grid">
              <div><Boxes size={17} /><span>Mempool</span><strong>{status?.mempool_size ?? 0}</strong></div>
              <div><Cpu size={17} /><span>CPU</span><strong>{state.snapshot?.core_cpu == null ? "Unavailable" : `${state.snapshot.core_cpu.toFixed(1)}%`}</strong></div>
              <div><MemoryStick size={17} /><span>Memory</span><strong>{bytes(state.snapshot?.core_memory_bytes)}</strong></div>
              <div><HardDrive size={17} /><span>Data</span><strong>{bytes(state.snapshot?.data_dir_size_bytes)}</strong></div>
              <div><TerminalSquare size={17} /><span>Logs</span><strong>{bytes(state.snapshot?.log_dir_size_bytes)}</strong></div>
            </div>
          </section>

          <section className="dashboard-operation-card dashboard-support-card" aria-labelledby="dashboard-support-title">
            <div className="dashboard-card-heading">
              <span className="dashboard-card-icon is-secure">
                <FileArchive size={20} aria-hidden="true" />
              </span>
              <div>
                <span>Desktop-managed paths</span>
                <h3 id="dashboard-support-title">Operator support</h3>
              </div>
              <span className="dashboard-status-badge is-neutral">{state.mockMode ? "Locked in mock" : "Available"}</span>
            </div>
            <div className="dashboard-support-actions">
              <button onClick={actions.generateSupportPackage} disabled={state.mockMode}>
                <FileArchive size={17} aria-hidden="true" />
                <span><strong>Generate support package</strong><small>Existing redacted Desktop bundle</small></span>
              </button>
              <button onClick={actions.openLogsDirectory} disabled={state.mockMode}>
                <FolderOpen size={17} aria-hidden="true" />
                <span><strong>Open logs directory</strong><small>Fixed Desktop-managed location</small></span>
              </button>
              <button onClick={actions.openDataDirectory} disabled={state.mockMode}>
                <FolderOpen size={17} aria-hidden="true" />
                <span><strong>Open data directory</strong><small>Fixed Desktop-managed location</small></span>
              </button>
            </div>
            <p className="dashboard-card-boundary">
              No arbitrary file browsing, shell execution, live-log stream, or Core protocol action is introduced.
            </p>
          </section>
        </div>
      </section>
    </div>
  );
}
