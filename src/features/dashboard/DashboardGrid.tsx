import { Activity, Database, FileArchive, FolderOpen, Network, Play } from "lucide-react";
import { Card } from "../../components/Card";
import { Metric } from "../../components/Metric";
import type { DesktopActions, DesktopState } from "../../state/desktopState";
import { bytes, shortHash } from "../../utils/format";
import { NetworkOverview } from "./NetworkOverview";

type DashboardGridProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function DashboardGrid({ state, actions }: DashboardGridProps) {
  const status = state.snapshot?.status;
  const mining = state.snapshot?.mining ?? status?.mining;
  const recovery = status?.recovery;
  const miningAvailable = state.snapshot?.mining?.enabled ?? status?.mining.available ?? false;

  return (
    <div className="dashboard-layout">
      <NetworkOverview state={state} actions={actions} />
      <div className="grid">
        <Card title="Core Process" icon={<Activity size={20} />}>
          <Metric label="State" value={state.snapshot?.process_state ?? "Unknown"} />
          <Metric label="PID" value={state.process?.pid ?? "Not running"} />
          <Metric label="API port" value={state.process?.api_port ?? "Private loopback"} />
          <Metric label="P2P port" value={state.process?.p2p_port ?? "Not listening"} />
        </Card>
        <Card title="Chain" icon={<Database size={20} />}>
          <Metric label="Height" value={status?.canonical_tip_height ?? "Unavailable"} />
          <Metric
            label="Tip"
            value={<span title={status?.canonical_tip_hash ?? undefined}>{shortHash(status?.canonical_tip_hash)}</span>}
          />
          <Metric label="Work" value={recovery?.local_work ?? "Unavailable"} />
          <Metric
            label="State root"
            value={<span title={status?.cached_state_root ?? undefined}>{shortHash(status?.cached_state_root)}</span>}
          />
        </Card>
        <Card title="Network" icon={<Network size={20} />}>
          <Metric label="Peers" value={status?.peer_count ?? 0} />
          <Metric label="Durable" value={status?.durable_peer_count ?? 0} />
          <Metric label="Inbound" value={status?.active_inbound_sessions ?? 0} />
          <Metric label="Outbound" value={status?.active_outbound_sessions ?? 0} />
          <Metric label="Transient" value={status?.transient_peer_count ?? 0} />
        </Card>
        <Card title="Mining And Recovery" icon={<Play size={20} />}>
          <Metric label="Mining available" value={String(miningAvailable)} />
          <Metric label="Mining active" value={String(mining?.active ?? false)} />
          <Metric label="Paused reason" value={mining?.paused_reason ?? "None"} />
          <Metric label="Recovery" value={recovery?.state ?? "Unknown"} />
        </Card>
        <Card title="Mempool And Resources" icon={<Activity size={20} />}>
          <Metric label="Mempool" value={status?.mempool_size ?? 0} />
          <Metric label="CPU" value={state.snapshot?.core_cpu == null ? "Unavailable" : `${state.snapshot.core_cpu.toFixed(1)}%`} />
          <Metric label="Memory" value={bytes(state.snapshot?.core_memory_bytes)} />
          <Metric label="Data" value={bytes(state.snapshot?.data_dir_size_bytes)} />
          <Metric label="Logs" value={bytes(state.snapshot?.log_dir_size_bytes)} />
        </Card>
        <Card title="Support" icon={<FileArchive size={20} />}>
          <div className="button-stack">
            <button onClick={actions.generateSupportPackage} disabled={state.mockMode}>
              <FileArchive size={18} />Generate Support Package
            </button>
            <button onClick={actions.openLogsDirectory} disabled={state.mockMode}>
              <FolderOpen size={18} />View Logs
            </button>
            <button onClick={actions.openDataDirectory} disabled={state.mockMode}>
              <FolderOpen size={18} />Open Data Directory
            </button>
          </div>
        </Card>
      </div>
    </div>
  );
}
