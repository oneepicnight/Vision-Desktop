import { FileText, FolderOpen, ShieldCheck, TerminalSquare } from "lucide-react";
import { Card } from "../../components/Card";
import { Metric } from "../../components/Metric";
import type { DesktopActions, DesktopState } from "../../state/desktopState";
import { deriveDiagnosticsViewModel } from "./diagnosticsStatus";

type DiagnosticsPanelProps = {
  state: DesktopState;
  actions: DesktopActions;
};

function renderLogTail(text: string | null) {
  if (text == null || text.trim().length === 0) {
    return <p className="empty-state">No log data available.</p>;
  }

  return <pre className="log-tail">{text}</pre>;
}

export function DiagnosticsPanel({ state, actions }: DiagnosticsPanelProps) {
  const viewModel = deriveDiagnosticsViewModel(state);
  const diagnostics = state.diagnostics;

  return (
    <div className="grid diagnostics-grid">
      <Card title="Diagnostics Status" icon={<ShieldCheck size={20} />}>
        <Metric label="Overall status" value={viewModel.overallStatus} />
        <Metric label="Process state" value={viewModel.processStatus} />
        <Metric label="Core API" value={viewModel.apiStatus} />
        <Metric label="Recovery state" value={viewModel.recoveryStatus} />
        <Metric label="Peer summary" value={viewModel.peerSummary} />
        <Metric label="Binary verification" value={viewModel.binaryVerification} />
        <Metric label="Mock mode" value={state.snapshot?.mock_mode ? "Yes" : "No"} />
        <Metric label="Last refresh" value={viewModel.lastRefresh} />
        <p className="note">{viewModel.summary}</p>
      </Card>

      <Card title="Desktop And Core Details" icon={<FileText size={20} />}>
        <Metric label="Manifest" value={viewModel.manifestSummary} />
        <Metric label="Executable path" value={viewModel.coreExecutablePath} />
        <Metric label="Data directory" value={viewModel.dataDirectory} />
        <Metric label="Log directory" value={viewModel.logDirectory} />
        <Metric label="Config path" value={viewModel.activeConfigPath} />
        <Metric label="API error" value={viewModel.apiError} />
        <Metric label="Operator message" value={viewModel.operatorMessage} />
        <p className="empty-state">
          This first Diagnostics page is read-only. It uses the existing Desktop
          snapshot, process metadata, binary verification, and fixed log-tail commands only.
        </p>
      </Card>

      <Card title="Support Package" icon={<FolderOpen size={20} />}>
        <Metric label="Availability" value={viewModel.supportPackageStatus} />
        <div className="button-stack">
          <button onClick={() => actions.generateSupportPackage()} disabled={state.loading}>
            Generate Support Package
          </button>
          <button onClick={() => actions.openLogsDirectory()} disabled={state.loading}>
            Open Logs Directory
          </button>
          <button onClick={() => actions.openDataDirectory()} disabled={state.loading}>
            Open Data Directory
          </button>
        </div>
      </Card>

      <Card title="Core Stdout Tail" icon={<TerminalSquare size={20} />}>
        {renderLogTail(diagnostics.stdoutTail)}
      </Card>

      <Card title="Core Stderr Tail" icon={<TerminalSquare size={20} />}>
        {renderLogTail(diagnostics.stderrTail)}
      </Card>
    </div>
  );
}
