import {
  ArrowDownLeft,
  ArrowUpRight,
  CircleDot,
  Network,
  Orbit,
  PlugZap,
  Radio,
  Route,
  Server,
  ShieldCheck,
  Waypoints,
} from "lucide-react";
import { Metric } from "../../components/Metric";
import type { DesktopActions, DesktopState } from "../../state/desktopState";

type PeerManagerPanelProps = {
  state: DesktopState;
  actions: DesktopActions;
};

function formatPeerAge(seconds?: number | null) {
  if (seconds == null) return "Unknown";
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h`;
}

export function PeerManagerPanel({ state }: PeerManagerPanelProps) {
  const status = state.snapshot?.status;
  const recovery = status?.recovery;
  const peers = [...(state.snapshot?.peers ?? [])].sort((left, right) => {
    if (right.height !== left.height) {
      return right.height - left.height;
    }
    return left.addr.localeCompare(right.addr);
  });
  const visibleConstellationNodes = peers.slice(0, 6);

  return (
    <div className="peer-command-center">
      <section className="peer-hero" aria-labelledby="peer-hero-title">
        <div className="peer-hero-copy">
          <div className="peer-hero-kicker">
            <Orbit size={15} aria-hidden="true" />
            Vision Peer Constellation
          </div>
          <h2 id="peer-hero-title">Network topology at a glance</h2>
          <p>
            Observe confirmed connection direction, reported height, freshness, and recovery context from one read-only surface.
          </p>

          <div className="peer-summary-strip" aria-label="Current peer summary">
            <div>
              <Network size={16} aria-hidden="true" />
              <span>Known</span>
              <strong>{status?.peer_count ?? peers.length}</strong>
            </div>
            <div>
              <ShieldCheck size={16} aria-hidden="true" />
              <span>Durable</span>
              <strong>{status?.durable_peer_count ?? 0}</strong>
            </div>
            <div>
              <ArrowDownLeft size={16} aria-hidden="true" />
              <span>Inbound</span>
              <strong>{status?.active_inbound_sessions ?? 0}</strong>
            </div>
            <div>
              <ArrowUpRight size={16} aria-hidden="true" />
              <span>Outbound</span>
              <strong>{status?.active_outbound_sessions ?? 0}</strong>
            </div>
          </div>
        </div>

        <div className="peer-constellation" aria-hidden="true">
          <div className="peer-constellation-orbit peer-constellation-orbit-outer" />
          <div className="peer-constellation-orbit peer-constellation-orbit-inner" />
          <div className="peer-constellation-core">
            <Waypoints size={40} />
          </div>
          <span className="peer-constellation-link peer-constellation-link-one" />
          <span className="peer-constellation-link peer-constellation-link-two" />
          <span className="peer-constellation-link peer-constellation-link-three" />
          {visibleConstellationNodes.map((peer, index) => (
            <span
              className={`peer-constellation-node peer-constellation-node-${index + 1}`}
              key={`${peer.addr}-${peer.height}-${peer.outbound}`}
            />
          ))}
          <div className="peer-constellation-count">
            <strong>{peers.length}</strong>
            <span>reported peers</span>
          </div>
        </div>

        <p className="peer-topology-disclaimer">
          Constellation positions are decorative topology markers, not geographic locations.
        </p>
      </section>

      <section className="peer-health-strip" aria-label="Peer connection context">
        <div className="peer-health-card">
          <Radio size={17} aria-hidden="true" />
          <span>Transient</span>
          <strong>{status?.transient_peer_count ?? 0}</strong>
        </div>
        <div className="peer-health-card">
          <Route size={17} aria-hidden="true" />
          <span>Dialable</span>
          <strong>{status?.dialable_peer_count ?? 0}</strong>
        </div>
        <div className="peer-health-card">
          <PlugZap size={17} aria-hidden="true" />
          <span>Recovery</span>
          <strong>{recovery?.state ?? "Unknown"}</strong>
        </div>
        <div className="peer-health-card">
          <Server size={17} aria-hidden="true" />
          <span>Remote height</span>
          <strong>{recovery?.remote_height ?? "Unavailable"}</strong>
        </div>
        <div className="peer-health-card">
          <CircleDot size={17} aria-hidden="true" />
          <span>Data source</span>
          <strong>{state.mockMode ? "Mock snapshot" : "Core snapshot"}</strong>
        </div>
      </section>

      <div className="peer-content-grid">
        <section className="peer-directory-panel" aria-labelledby="peer-directory-title">
          <div className="peer-section-heading">
            <span className="peer-section-icon">
              <Network size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="peer-directory-title">Connected peer directory</h3>
              <p>Sorted by reported chain height, then exact peer address.</p>
            </div>
            <span className="peer-count-badge">{peers.length} reported</span>
          </div>

          {peers.length === 0 ? (
            <div className="peer-directory-empty">
              <span className="peer-empty-orbit" aria-hidden="true">
                <Network size={30} />
              </span>
              <div>
                <strong>No peers reported</strong>
                <p>Connections will appear here when they are present in the shared Desktop snapshot.</p>
              </div>
            </div>
          ) : (
            <div className="peer-directory-list">
              {peers.map((peer, index) => (
                <article className="peer-directory-row" key={`${peer.addr}-${peer.height}-${peer.outbound}`}>
                  <div className="peer-directory-rank" aria-label={`Peer ${index + 1}`}>
                    {String(index + 1).padStart(2, "0")}
                  </div>
                  <div className="peer-directory-identity">
                    <span className="peer-address-label">Peer address</span>
                    <strong title={peer.addr}>{peer.addr}</strong>
                    <span className={`peer-direction ${peer.outbound ? "is-outbound" : "is-inbound"}`}>
                      {peer.outbound ? <ArrowUpRight size={13} /> : <ArrowDownLeft size={13} />}
                      {peer.outbound ? "Outbound" : "Inbound"}
                    </span>
                  </div>
                  <div className="peer-directory-metric">
                    <span>State</span>
                    <strong>{peer.state}</strong>
                  </div>
                  <div className="peer-directory-metric">
                    <span>Height</span>
                    <strong>{peer.height}</strong>
                  </div>
                  <div className="peer-directory-metric">
                    <span>Height age</span>
                    <strong>{formatPeerAge(peer.height_age_secs)}</strong>
                  </div>
                </article>
              ))}
            </div>
          )}
        </section>

        <section className="peer-recovery-panel" aria-labelledby="peer-recovery-title">
          <div className="peer-section-heading">
            <span className="peer-section-icon peer-recovery-icon">
              <PlugZap size={20} aria-hidden="true" />
            </span>
            <div>
              <h3 id="peer-recovery-title">Recovery context</h3>
              <p>Existing recovery observations only.</p>
            </div>
          </div>
          <Metric label="Recovery state" value={recovery?.state ?? "Unknown"} />
          <Metric label="Peer address" value={recovery?.peer_addr ?? "None"} />
          <Metric label="Remote height" value={recovery?.remote_height ?? "Unavailable"} />
          <Metric label="Remote work" value={recovery?.remote_work ?? "Unavailable"} />
          <Metric label="Remote tip" value={recovery?.remote_tip_hash ?? "Unavailable"} />
          <p className="peer-recovery-note">
            Desktop does not derive trust, latency, reputation, geography, or routing scores from these values.
          </p>
        </section>
      </div>
    </div>
  );
}
