import { FolderTree, HardDrive, Info, Network, ShieldCheck } from "lucide-react";
import { Card } from "../../components/Card";
import { Metric } from "../../components/Metric";
import type { DesktopState } from "../../state/desktopState";
import { deriveConfigurationViewModel, type ConfigurationEntry } from "./configurationStatus";

type ConfigurationPanelProps = {
  state: DesktopState;
};

function renderEntries(entries: ConfigurationEntry[]) {
  return entries.map((entry) => (
    <div key={entry.label} className="config-entry">
      <div className="config-entry-header">
        <strong>{entry.label}</strong>
      </div>
      <div className="config-entry-grid">
        <span>Configured</span>
        <strong>{entry.configuredValue}</strong>
        <em>{entry.configuredSource}</em>
        <span>Runtime</span>
        <strong>{entry.runtimeValue}</strong>
        <em>{entry.runtimeSource}</em>
      </div>
      {entry.note ? <p className="empty-state">{entry.note}</p> : null}
    </div>
  ));
}

export function ConfigurationPanel({ state }: ConfigurationPanelProps) {
  const viewModel = deriveConfigurationViewModel(state);

  return (
    <div className="grid configuration-grid">
      <Card title="Configuration Status" icon={<ShieldCheck size={20} />}>
        <Metric label="Overall status" value={viewModel.overallStatus} />
        <Metric label="Configuration source" value={viewModel.sourceStatus} />
        <Metric label="Config path" value={viewModel.sourcePath} />
        <Metric label="Validation state" value={viewModel.validationState} />
        <Metric label="Mock mode" value={viewModel.mockMode} />
        <Metric label="Last refresh" value={viewModel.lastRefresh} />
        <Metric label="Configured/runtime comparison" value={viewModel.mismatchSummary} />
        <p className="note">{viewModel.summary}</p>
        <p className="empty-state">
          This page is read-only. It shows Desktop-managed configuration values plus limited
          runtime observations that are already exposed by the current Desktop snapshot and
          process-state models.
        </p>
      </Card>

      <Card title="General" icon={<Info size={20} />}>
        {renderEntries(viewModel.generalEntries)}
      </Card>

      <Card title="Paths" icon={<HardDrive size={20} />}>
        {renderEntries(viewModel.pathEntries)}
      </Card>

      <Card title="Network" icon={<Network size={20} />}>
        {renderEntries(viewModel.networkEntries)}
      </Card>

      <Card title="Peers" icon={<FolderTree size={20} />}>
        {renderEntries(viewModel.peerEntries)}
      </Card>

      <Card title="Mining" icon={<ShieldCheck size={20} />}>
        {renderEntries(viewModel.miningEntries)}
        <div className="config-limitations">
          {viewModel.limitations.map((limitation) => (
            <p key={limitation} className="empty-state">
              {limitation}
            </p>
          ))}
        </div>
      </Card>
    </div>
  );
}
