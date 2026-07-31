import {
  Activity,
  AlertTriangle,
  Binary,
  Clock3,
  FileArchive,
  FileText,
  FolderOpen,
  HeartPulse,
  Radio,
  Server,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react";
import { Metric } from "../../components/Metric";
import type { DesktopActions, DesktopState } from "../../state/desktopState";
import { deriveDiagnosticsViewModel } from "./diagnosticsStatus";

type DiagnosticsPanelProps = {
  state: DesktopState;
  actions: DesktopActions;
};

function getDiagnosticsTone(status: string) {
  if (status === "Healthy") return "is-healthy";
  if (status === "Degraded" || status === "Recovery mode") return "is-warning";
  if (status === "Unavailable") return "is-unavailable";
  if (status === "Mock mode") return "is-mock";
  return "is-unknown";
}

function renderLogTail(text: string | null, source: string) {
  if (text == null || text.trim().length === 0) {
    return (
      <div className="diagnostics-log-empty">
        <TerminalSquare size={24} aria-hidden="true" />
        <strong>No {source} data available</strong>
        <span>The existing fixed Desktop-managed log tail returned no content.</span>
      </div>
    );
  }

  return <pre className="diagnostics-log-tail">{text}</pre>;
}

export function DiagnosticsPanel({ state, actions }: DiagnosticsPanelProps) {
  const viewModel = deriveDiagnosticsViewModel(state);
  const diagnostics = state.diagnostics;
  const statusTone = getDiagnosticsTone(viewModel.overallStatus);

  return (
    <div className="diagnostics-command-center">
      <section className={`diagnostics-hero ${statusTone}`} aria-labelledby="diagnostics-hero-title">
        <div className="diagnostics-hero-copy">
          <div className="diagnostics-hero-kicker">
            <HeartPulse size={15} aria-hidden="true" />
            Vision Systems Observatory
          </div>
          <div className="diagnostics-hero-heading">
            <div>
              <span>Current diagnostics classification</span>
              <h2 id="diagnostics-hero-title">{viewModel.overallStatus}</h2>
            </div>
            <span className="diagnostics-source-badge">
              <ShieldCheck size={13} aria-hidden="true" />
              Fixed sources
            </span>
          </div>
          <p>{viewModel.summary}</p>

          <div className="diagnostics-health-strip" aria-label="Current diagnostics summary">
            <div>
              <Server size={16} aria-hidden="true" />
              <span>Core process</span>
              <strong>{viewModel.processStatus}</strong>
            </div>
            <div>
              <Radio size={16} aria-hidden="true" />
              <span>Private API</span>
              <strong>{viewModel.apiStatus}</strong>
            </div>
            <div>
              <Activity size={16} aria-hidden="true" />
              <span>Recovery</span>
              <strong>{viewModel.recoveryStatus}</strong>
            </div>
            <div>
              <Binary size={16} aria-hidden="true" />
              <span>Core binary</span>
              <strong>{viewModel.binaryVerification}</strong>
            </div>
          </div>
        </div>

        <div className="diagnostics-radar" aria-hidden="true">
          <div className="diagnostics-radar-ring diagnostics-radar-ring-outer" />
          <div className="diagnostics-radar-ring diagnostics-radar-ring-middle" />
          <div className="diagnostics-radar-ring diagnostics-radar-ring-inner" />
          <div className="diagnostics-radar-sweep" />
          <span className="diagnostics-radar-node diagnostics-radar-node-one" />
          <span className="diagnostics-radar-node diagnostics-radar-node-two" />
          <span className="diagnostics-radar-node diagnostics-radar-node-three" />
          <div className="diagnostics-radar-core">
            <HeartPulse size={42} />
          </div>
          <div className="diagnostics-radar-caption">
            <strong>{viewModel.peerSummary}</strong>
            <span>snapshot context</span>
          </div>
        </div>
      </section>

      <div className="diagnostics-content-grid">
        <section className="diagnostics-system-panel" aria-labelledby="diagnostics-system-title">
          <div className="diagnostics-section-heading">
            <span className="diagnostics-section-icon">
              <FileText size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="diagnostics-system-title">Desktop and Core identity</h3>
              <p>Existing process, manifest, verification, and snapshot metadata.</p>
            </div>
            <span className={`diagnostics-status-pill ${statusTone}`}>{viewModel.overallStatus}</span>
          </div>
          <div className="diagnostics-metric-grid">
            <Metric label="Manifest" value={viewModel.manifestSummary} />
            <Metric label="Core executable" value={viewModel.coreExecutablePath} />
            <Metric label="Data directory" value={viewModel.dataDirectory} />
            <Metric label="Log directory" value={viewModel.logDirectory} />
            <Metric label="Config path" value={viewModel.activeConfigPath} />
            <Metric label="Peer summary" value={viewModel.peerSummary} />
            <Metric label="API error" value={viewModel.apiError} />
            <Metric label="Operator message" value={viewModel.operatorMessage} />
          </div>
        </section>

        <section className="diagnostics-support-panel" aria-labelledby="diagnostics-support-title">
          <div className="diagnostics-section-heading">
            <span className="diagnostics-section-icon diagnostics-support-icon">
              <FileArchive size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="diagnostics-support-title">Operator support tools</h3>
              <p>Established Desktop-owned actions only.</p>
            </div>
          </div>
          <Metric label="Support package" value={viewModel.supportPackageStatus} />
          <Metric label="Mock mode" value={state.mockMode || state.snapshot?.mock_mode ? "Yes" : "No"} />
          <Metric label="Last refresh" value={viewModel.lastRefresh} />
          <div className="diagnostics-action-stack">
            <button onClick={() => actions.generateSupportPackage()} disabled={state.loading}>
              <FileArchive size={16} aria-hidden="true" />
              Generate Support Package
            </button>
            <button onClick={() => actions.openLogsDirectory()} disabled={state.loading}>
              <FolderOpen size={16} aria-hidden="true" />
              Open Logs Directory
            </button>
            <button onClick={() => actions.openDataDirectory()} disabled={state.loading}>
              <FolderOpen size={16} aria-hidden="true" />
              Open Data Directory
            </button>
          </div>
          <p className="diagnostics-support-note">
            These actions use existing Desktop-managed paths. No arbitrary path browsing, shell execution, or Core write command is added here.
          </p>
        </section>

        <section className="diagnostics-alert-panel" aria-labelledby="diagnostics-alert-title">
          <div className="diagnostics-section-heading">
            <span className="diagnostics-section-icon diagnostics-alert-icon">
              <AlertTriangle size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="diagnostics-alert-title">Current operator context</h3>
              <p>Errors are displayed as received through the existing state boundary.</p>
            </div>
          </div>
          <div className={`diagnostics-context-message ${statusTone}`}>
            <strong>{viewModel.overallStatus}</strong>
            <p>{viewModel.summary}</p>
          </div>
          <Metric label="API status" value={viewModel.apiStatus} />
          <Metric label="Recovery state" value={viewModel.recoveryStatus} />
          <Metric label="Binary verification" value={viewModel.binaryVerification} />
        </section>
      </div>

      <section className="diagnostics-logs-console" aria-labelledby="diagnostics-logs-title">
        <div className="diagnostics-section-heading diagnostics-logs-heading">
          <span className="diagnostics-section-icon diagnostics-terminal-icon">
            <TerminalSquare size={20} aria-hidden="true" />
          </span>
          <div>
            <h3 id="diagnostics-logs-title">Core log console</h3>
            <p>Recent fixed stdout and stderr tails from Desktop-managed log files.</p>
          </div>
          <span className="diagnostics-log-boundary">
            <Clock3 size={13} aria-hidden="true" />
            Snapshot tails
          </span>
        </div>

        <div className="diagnostics-log-grid">
          <article className="diagnostics-log-panel">
            <header>
              <span className="diagnostics-terminal-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
              <strong>Core stdout</strong>
              <small>Fixed tail</small>
            </header>
            {renderLogTail(diagnostics.stdoutTail, "stdout")}
          </article>

          <article className="diagnostics-log-panel diagnostics-log-panel-error">
            <header>
              <span className="diagnostics-terminal-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
              <strong>Core stderr</strong>
              <small>Fixed tail</small>
            </header>
            {renderLogTail(diagnostics.stderrTail, "stderr")}
          </article>
        </div>

        <p className="diagnostics-logs-boundary-note">
          No live stream, arbitrary log selection, client-side export, invented severity classification, or unrestricted filesystem access is provided.
        </p>
      </section>
    </div>
  );
}
