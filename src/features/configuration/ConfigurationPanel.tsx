import type { ReactNode } from "react";
import {
  Braces,
  CircleDotDashed,
  FolderTree,
  GitCompareArrows,
  HardDrive,
  Info,
  LockKeyhole,
  Network,
  Pickaxe,
  ServerCog,
  ShieldCheck,
  Workflow,
} from "lucide-react";
import { Metric } from "../../components/Metric";
import type { DesktopState } from "../../state/desktopState";
import { deriveConfigurationViewModel, type ConfigurationEntry } from "./configurationStatus";

type ConfigurationPanelProps = {
  state: DesktopState;
};

type ConfigurationSectionProps = {
  title: string;
  description: string;
  icon: ReactNode;
  tone?: string;
  entries: ConfigurationEntry[];
};

function getConfigurationTone(status: string) {
  if (status === "Configuration available" || status === "Configuration loaded") {
    return "is-available";
  }
  if (status === "Configured/runtime mismatch") return "is-warning";
  if (status === "Configuration invalid") return "is-invalid";
  if (status === "Mock mode") return "is-mock";
  return "is-unavailable";
}

function ConfigurationEntryCard({ entry }: { entry: ConfigurationEntry }) {
  return (
    <article className="configuration-entry-card">
      <header>{entry.label}</header>
      <div className="configuration-entry-comparison">
        <div>
          <span>Configured</span>
          <strong>{entry.configuredValue}</strong>
          <small>{entry.configuredSource}</small>
        </div>
        <GitCompareArrows size={15} aria-hidden="true" />
        <div>
          <span>Runtime</span>
          <strong>{entry.runtimeValue}</strong>
          <small>{entry.runtimeSource}</small>
        </div>
      </div>
      {entry.note ? <p>{entry.note}</p> : null}
    </article>
  );
}

function ConfigurationSection({
  title,
  description,
  icon,
  tone = "",
  entries,
}: ConfigurationSectionProps) {
  return (
    <section className="configuration-section-panel" aria-labelledby={`configuration-${title.toLowerCase()}-title`}>
      <div className="configuration-section-heading">
        <span className={`configuration-section-icon ${tone}`}>{icon}</span>
        <div>
          <h3 id={`configuration-${title.toLowerCase()}-title`}>{title}</h3>
          <p>{description}</p>
        </div>
        <span className="configuration-entry-count">{entries.length} fields</span>
      </div>
      <div className="configuration-entry-list">
        {entries.map((entry) => (
          <ConfigurationEntryCard entry={entry} key={entry.label} />
        ))}
      </div>
    </section>
  );
}

export function ConfigurationPanel({ state }: ConfigurationPanelProps) {
  const viewModel = deriveConfigurationViewModel(state);
  const statusTone = getConfigurationTone(viewModel.overallStatus);

  return (
    <div className="configuration-command-center">
      <section className={`configuration-hero ${statusTone}`} aria-labelledby="configuration-hero-title">
        <div className="configuration-hero-copy">
          <div className="configuration-hero-kicker">
            <Braces size={15} aria-hidden="true" />
            Vision Node Blueprint
          </div>
          <div className="configuration-hero-heading">
            <div>
              <span>Desktop-managed configuration</span>
              <h2 id="configuration-hero-title">{viewModel.overallStatus}</h2>
            </div>
            <span className="configuration-readonly-badge">
              <ShieldCheck size={13} aria-hidden="true" />
              Read-only
            </span>
          </div>
          <p>{viewModel.summary}</p>

          <div className="configuration-status-strip" aria-label="Current configuration summary">
            <div>
              <FolderTree size={16} aria-hidden="true" />
              <span>Source</span>
              <strong>{viewModel.sourceStatus}</strong>
            </div>
            <div>
              <ShieldCheck size={16} aria-hidden="true" />
              <span>Validation</span>
              <strong>{viewModel.validationState}</strong>
            </div>
            <div>
              <CircleDotDashed size={16} aria-hidden="true" />
              <span>Mock mode</span>
              <strong>{viewModel.mockMode}</strong>
            </div>
            <div>
              <GitCompareArrows size={16} aria-hidden="true" />
              <span>Comparison</span>
              <strong>{viewModel.mismatchSummary}</strong>
            </div>
          </div>
        </div>

        <div className="configuration-blueprint" aria-hidden="true">
          <div className="configuration-blueprint-frame configuration-blueprint-frame-outer" />
          <div className="configuration-blueprint-frame configuration-blueprint-frame-inner" />
          <span className="configuration-blueprint-line configuration-blueprint-line-one" />
          <span className="configuration-blueprint-line configuration-blueprint-line-two" />
          <span className="configuration-blueprint-node configuration-blueprint-node-one" />
          <span className="configuration-blueprint-node configuration-blueprint-node-two" />
          <span className="configuration-blueprint-node configuration-blueprint-node-three" />
          <div className="configuration-blueprint-core">
            <ServerCog size={42} />
          </div>
          <div className="configuration-blueprint-caption">
            <strong>{viewModel.mockMode === "Yes" ? "Mock context" : "Desktop source"}</strong>
            <span>configured and observed</span>
          </div>
        </div>
      </section>

      <section className="configuration-source-panel" aria-labelledby="configuration-source-title">
        <div className="configuration-section-heading">
          <span className="configuration-section-icon">
            <Workflow size={20} aria-hidden="true" />
          </span>
          <div>
            <h3 id="configuration-source-title">Configuration provenance</h3>
            <p>Where Desktop loaded the values and what it can safely compare.</p>
          </div>
          <span className={`configuration-status-pill ${statusTone}`}>{viewModel.overallStatus}</span>
        </div>
        <div className="configuration-source-grid">
          <div className="configuration-source-path">
            <span>Node configuration path</span>
            <code title={viewModel.sourcePath}>{viewModel.sourcePath}</code>
            <small>{viewModel.sourceStatus}</small>
          </div>
          <div className="configuration-source-metrics">
            <Metric label="Validation" value={viewModel.validationState} />
            <Metric label="Last refresh" value={viewModel.lastRefresh} />
            <Metric label="Mock mode" value={viewModel.mockMode} />
          </div>
        </div>
        <div className={`configuration-comparison-note ${statusTone}`}>
          <GitCompareArrows size={17} aria-hidden="true" />
          <div>
            <span>Configured/runtime comparison</span>
            <strong>{viewModel.mismatchSummary}</strong>
          </div>
        </div>
      </section>

      <div className="configuration-section-grid">
        <ConfigurationSection
          title="General"
          description="Node identity and observed process state."
          icon={<Info size={20} aria-hidden="true" />}
          entries={viewModel.generalEntries}
        />
        <ConfigurationSection
          title="Paths"
          description="Desktop-managed locations and limited runtime observations."
          icon={<HardDrive size={20} aria-hidden="true" />}
          tone="is-purple"
          entries={viewModel.pathEntries}
        />
        <ConfigurationSection
          title="Network"
          description="Configured ports and public endpoint presentation."
          icon={<Network size={20} aria-hidden="true" />}
          tone="is-cyan"
          entries={viewModel.networkEntries}
        />
        <ConfigurationSection
          title="Peers"
          description="Configured seeds remain distinct from observed peers."
          icon={<FolderTree size={20} aria-hidden="true" />}
          tone="is-gold"
          entries={viewModel.peerEntries}
        />
        <ConfigurationSection
          title="Mining"
          description="Public mining configuration and confirmed runtime state."
          icon={<Pickaxe size={20} aria-hidden="true" />}
          tone="is-violet"
          entries={viewModel.miningEntries}
        />

        <section className="configuration-boundary-panel" aria-labelledby="configuration-boundary-title">
          <div className="configuration-section-heading">
            <span className="configuration-section-icon is-secure">
              <LockKeyhole size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="configuration-boundary-title">Read-only security boundary</h3>
              <p>No configuration editor or secret-bearing field is present.</p>
            </div>
          </div>
          <ul>
            {viewModel.limitations.map((limitation) => (
              <li key={limitation}>{limitation}</li>
            ))}
          </ul>
          <p>
            Desktop does not import the legacy endpoint editor, browser storage, key export, mnemonic display, private-key display, or wallet-wipe behavior.
          </p>
        </section>
      </div>
    </div>
  );
}
