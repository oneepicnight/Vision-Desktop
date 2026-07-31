import {
  Braces,
  ChevronDown,
  ChevronUp,
  CircleDotDashed,
  Globe2,
  Network,
  Pickaxe,
  Save,
  ServerCog,
  ShieldCheck,
  WalletCards,
} from "lucide-react";
import type { DesktopActions } from "../../state/desktopState";
import type { NodeConfig, NodeMode } from "../../types/core";

type CreateNodeWizardProps = {
  wizardOpen: boolean;
  config: NodeConfig;
  onWizardOpenChange: (open: boolean) => void;
  onConfigChange: (config: NodeConfig) => void;
  actions: DesktopActions;
};

export function CreateNodeWizard({
  wizardOpen,
  config,
  onWizardOpenChange,
  onConfigChange,
  actions,
}: CreateNodeWizardProps) {
  return (
    <section className={`node-wizard ${wizardOpen ? "is-open" : ""}`} aria-labelledby="node-wizard-title">
      <div className="node-wizard-header">
        <span className="node-wizard-header-icon">
          <ServerCog size={22} aria-hidden="true" />
        </span>
        <div>
          <span className="node-wizard-kicker">Desktop configuration workflow</span>
          <h2 id="node-wizard-title">Create node configuration</h2>
          <p>Prepare the Desktop-managed node settings without starting Core automatically.</p>
        </div>
        <div className="node-wizard-summary" aria-label="Current node configuration summary">
          <span>{config.mode}</span>
          <span>P2P {config.p2p_port}</span>
          <span>{config.mining_enabled ? "Mining enabled" : "Mining disabled"}</span>
        </div>
        <button
          className="node-wizard-toggle"
          type="button"
          onClick={() => onWizardOpenChange(!wizardOpen)}
          aria-expanded={wizardOpen}
        >
          {wizardOpen ? <ChevronUp size={17} aria-hidden="true" /> : <ChevronDown size={17} aria-hidden="true" />}
          {wizardOpen ? "Hide setup" : "Open setup"}
        </button>
      </div>

      {wizardOpen ? (
        <div className="node-wizard-body">
          <div className="node-wizard-section-heading">
            <Braces size={18} aria-hidden="true" />
            <div>
              <h3>Node identity and network</h3>
              <p>Values are saved through the existing typed Desktop configuration action.</p>
            </div>
          </div>

          <div className="node-wizard-grid">
            <label className="node-wizard-field">
              <span>
                <ServerCog size={14} aria-hidden="true" />
                Node name
              </span>
              <input
                value={config.node_name}
                onChange={(event) => onConfigChange({ ...config, node_name: event.target.value })}
              />
              <small>Local operator label managed by Vision Desktop.</small>
            </label>

            <label className="node-wizard-field">
              <span>
                <CircleDotDashed size={14} aria-hidden="true" />
                Mode
              </span>
              <select
                value={config.mode}
                onChange={(event) => onConfigChange({ ...config, mode: event.target.value as NodeMode })}
              >
                <option>LocalTesting</option>
                <option>PrivateNetwork</option>
                <option>InternetNetwork</option>
              </select>
              <small>Select only a mode already supported by the Desktop configuration model.</small>
            </label>

            <label className="node-wizard-field">
              <span>
                <Network size={14} aria-hidden="true" />
                P2P port
              </span>
              <input
                type="number"
                value={config.p2p_port}
                onChange={(event) => onConfigChange({ ...config, p2p_port: Number(event.target.value) })}
              />
              <small>Configured listener port; this form does not modify firewall or router settings.</small>
            </label>

            <label className="node-wizard-field">
              <span>
                <Globe2 size={14} aria-hidden="true" />
                Advertised address
              </span>
              <input
                value={config.advertised_host ?? ""}
                placeholder="public host or DNS"
                onChange={(event) => onConfigChange({ ...config, advertised_host: event.target.value || null })}
              />
              <small>Preserved as configured text; Desktop does not resolve or probe this address here.</small>
            </label>

            <label className="node-wizard-field node-wizard-field-wide">
              <span>
                <Network size={14} aria-hidden="true" />
                Seed peers
              </span>
              <input
                defaultValue={config.seed_peers.join(", ")}
                placeholder="host:port, host:port"
                onChange={(event) =>
                  onConfigChange({
                    ...config,
                    seed_peers: event.target.value
                      .split(",")
                      .map((value) => value.trim())
                      .filter(Boolean),
                  })
                }
              />
              <small>Comma-separated configured peers; no connectivity test is performed from this form.</small>
            </label>
          </div>

          <div className="node-wizard-section-heading node-wizard-mining-heading">
            <Pickaxe size={18} aria-hidden="true" />
            <div>
              <h3>Mining configuration</h3>
              <p>Configured status is separate from observed runtime activity.</p>
            </div>
          </div>

          <div className="node-wizard-mining-grid">
            <label className="node-wizard-mining-toggle">
              <input
                type="checkbox"
                checked={config.mining_enabled}
                onChange={(event) => onConfigChange({ ...config, mining_enabled: event.target.checked })}
              />
              <span className="node-wizard-toggle-track" aria-hidden="true" />
              <span>
                <strong>Mining configured</strong>
                <small>{config.mining_enabled ? "Enabled in Desktop configuration" : "Disabled in Desktop configuration"}</small>
              </span>
            </label>

            <label className="node-wizard-field node-wizard-reward-field">
              <span>
                <WalletCards size={14} aria-hidden="true" />
                Mining reward address
              </span>
              <input
                value={config.miner_reward_address}
                onChange={(event) => onConfigChange({ ...config, miner_reward_address: event.target.value })}
              />
              <small>Public configured value only; this workflow does not prove wallet custody.</small>
            </label>
          </div>

          <div className="node-wizard-safety-note">
            <ShieldCheck size={19} aria-hidden="true" />
            <div>
              <strong>Explicit operator boundary</strong>
              <p>
                Internet mode may require manual router forwarding in RC2. Vision Desktop will not open firewall or router ports, test arbitrary endpoints, or start Core automatically when this configuration is saved.
              </p>
            </div>
          </div>

          <button className="node-wizard-save" type="button" onClick={actions.saveNodeConfig}>
            <Save size={18} aria-hidden="true" />
            Save node configuration
          </button>
        </div>
      ) : null}
    </section>
  );
}
