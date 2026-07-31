import { Activity, Flame, Info } from "lucide-react";
import { Card } from "../../components/Card";
import { Metric } from "../../components/Metric";
import type { DesktopState } from "../../state/desktopState";
import { deriveMiningViewModel } from "./miningStatus";

type MiningPanelProps = {
  state: DesktopState;
};

export function MiningPanel({ state }: MiningPanelProps) {
  const snapshot = state.snapshot;
  const viewModel = deriveMiningViewModel(state);

  return (
    <div className="grid mining-grid">
      <Card title="Mining Status" icon={<Flame size={20} />}>
        <Metric label="Status" value={viewModel.headline} />
        <Metric label="Runtime enabled" value={viewModel.runtimeEnabled} />
        <Metric label="Activity" value={viewModel.activity} />
        <Metric label="Availability" value={viewModel.availability} />
        <Metric label="Process state" value={viewModel.processReadiness} />
        <p className="note">{viewModel.detail}</p>
      </Card>

      <Card title="Mining Context" icon={<Activity size={20} />}>
        <Metric label="Height context" value={viewModel.heightContext} />
        <Metric label="Recovery state" value={viewModel.recoveryState} />
        <Metric label="Paused reason" value={viewModel.pausedReason} />
        <Metric
          label="Desktop config mining"
          value={state.config.mining_enabled ? "Enabled" : "Disabled"}
        />
        <Metric
          label="Desktop reward address"
          value={viewModel.rewardAddress}
        />
      </Card>

      <Card title="Snapshot Metadata" icon={<Info size={20} />}>
        <Metric label="Mock mode" value={snapshot?.mock_mode ? "Yes" : "No"} />
        <Metric label="Last refresh" value={viewModel.lastUpdated} />
        <Metric
          label="Chain height"
          value={snapshot?.status?.canonical_tip_height ?? "Unavailable"}
        />
        <Metric
          label="Core API status"
          value={snapshot?.api_error ?? "Connected"}
        />
        <p className="empty-state">
          This first Mining page is read-only. It reports existing Desktop snapshot fields only and does not imply that enabled mining is actively producing blocks.
        </p>
      </Card>
    </div>
  );
}
