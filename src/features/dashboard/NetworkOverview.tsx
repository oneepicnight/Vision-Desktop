import { ArrowRight, CircleDot, Network, Orbit } from "lucide-react";
import type { DesktopActions, DesktopState } from "../../state/desktopState";

type NetworkOverviewProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function NetworkOverview({ state, actions }: NetworkOverviewProps) {
  const status = state.snapshot?.status;
  const processState = state.snapshot?.process_state ?? "Unknown";
  const recoveryState = status?.recovery.state ?? "Unknown";
  const chainHeight = status?.canonical_tip_height ?? "Unavailable";
  const peerCount = status?.peer_count ?? 0;

  return (
    <section className="network-overview" aria-labelledby="network-overview-title">
      <div className="network-overview-copy">
        <div className="network-overview-kicker">
          <Orbit size={15} aria-hidden="true" />
          Vision World Network
        </div>
        <h2 id="network-overview-title">Your window into the network</h2>
        <p>
          One operator view for Core health, chain progress, peer visibility, and recovery context.
        </p>

        <div className="network-overview-facts" aria-label="Current network summary">
          <div>
            <span>Core</span>
            <strong>{processState}</strong>
          </div>
          <div>
            <span>Chain height</span>
            <strong>{chainHeight}</strong>
          </div>
          <div>
            <span>Connected peers</span>
            <strong>{peerCount}</strong>
          </div>
          <div>
            <span>Recovery</span>
            <strong>{recoveryState}</strong>
          </div>
        </div>

        <button
          type="button"
          className="network-overview-action"
          onClick={() => actions.setActiveView("peers")}
        >
          <Network size={17} aria-hidden="true" />
          Open Peer Manager
          <ArrowRight size={16} aria-hidden="true" />
        </button>
      </div>

      <div className="network-visual" aria-hidden="true">
        <div className="network-visual-stars" />
        <div className="network-orbit network-orbit-outer">
          <i className="network-node network-node-one" />
          <i className="network-node network-node-two" />
        </div>
        <div className="network-orbit network-orbit-inner">
          <i className="network-node network-node-three" />
        </div>
        <div className="network-globe">
          <span className="network-globe-grid" />
          <span className="network-globe-land network-globe-land-one" />
          <span className="network-globe-land network-globe-land-two" />
          <span className="network-globe-shade" />
        </div>
        <div className="network-visual-label">
          <CircleDot size={13} />
          Decorative network view
        </div>
      </div>

      <p className="network-overview-disclaimer">
        Peer locations are not exposed; the globe is a visual network motif only.
      </p>
    </section>
  );
}
