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
    <section className="wizard">
      <div className="wizard-header">
        <h2>Create Node</h2>
        <button onClick={() => onWizardOpenChange(!wizardOpen)}>{wizardOpen ? "Hide" : "Open"}</button>
      </div>
      {wizardOpen && (
        <div className="wizard-grid">
          <label>
            Node name
            <input value={config.node_name} onChange={(event) => onConfigChange({ ...config, node_name: event.target.value })} />
          </label>
          <label>
            Mode
            <select
              value={config.mode}
              onChange={(event) => onConfigChange({ ...config, mode: event.target.value as NodeMode })}
            >
              <option>LocalTesting</option>
              <option>PrivateNetwork</option>
              <option>InternetNetwork</option>
            </select>
          </label>
          <label>
            P2P port
            <input
              type="number"
              value={config.p2p_port}
              onChange={(event) => onConfigChange({ ...config, p2p_port: Number(event.target.value) })}
            />
          </label>
          <label>
            Seed peers
            <input
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
          </label>
          <label>
            Advertised address
            <input
              placeholder="public host or DNS"
              onChange={(event) => onConfigChange({ ...config, advertised_host: event.target.value || null })}
            />
          </label>
          <label>
            Mining
            <input
              type="checkbox"
              checked={config.mining_enabled}
              onChange={(event) => onConfigChange({ ...config, mining_enabled: event.target.checked })}
            />
          </label>
          <label className="wide">
            Reward address
            <input
              value={config.miner_reward_address}
              onChange={(event) => onConfigChange({ ...config, miner_reward_address: event.target.value })}
            />
          </label>
          <p className="wide note">
            Internet mode requires manual router forwarding in RC2. Vision Desktop will not open firewall or router ports automatically.
          </p>
          <button className="wide" onClick={actions.saveNodeConfig}>
            Create Node
          </button>
        </div>
      )}
    </section>
  );
}
