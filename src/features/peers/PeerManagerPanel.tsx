import { Network, PlugZap, Radio } from "lucide-react";
import { Card } from "../../components/Card";
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

  return (
    <div className="grid peer-grid">
      <Card title="Peer Summary" icon={<Network size={20} />}>
        <Metric label="Known peers" value={status?.peer_count ?? peers.length} />
        <Metric label="Durable peers" value={status?.durable_peer_count ?? 0} />
        <Metric label="Inbound sessions" value={status?.active_inbound_sessions ?? 0} />
        <Metric label="Outbound sessions" value={status?.active_outbound_sessions ?? 0} />
        <Metric label="Transient peers" value={status?.transient_peer_count ?? 0} />
        <Metric label="Dialable peers" value={status?.dialable_peer_count ?? 0} />
      </Card>

      <Card title="Recovery Context" icon={<PlugZap size={20} />}>
        <Metric label="Recovery state" value={recovery?.state ?? "Unknown"} />
        <Metric label="Peer address" value={recovery?.peer_addr ?? "None"} />
        <Metric label="Remote height" value={recovery?.remote_height ?? "Unavailable"} />
        <Metric label="Remote work" value={recovery?.remote_work ?? "Unavailable"} />
        <Metric label="Remote tip" value={recovery?.remote_tip_hash ?? "Unavailable"} />
      </Card>

      <Card title="Peer Directory" icon={<Radio size={20} />}>
        {peers.length === 0 ? (
          <p className="empty-state">
            No peers are currently reported by Core. Once connections are available, they will appear here.
          </p>
        ) : (
          <div className="peer-list">
            {peers.map((peer) => (
              <article className="peer-row" key={`${peer.addr}-${peer.height}-${peer.outbound}`}>
                <div className="peer-row-header">
                  <strong title={peer.addr}>{peer.addr}</strong>
                  <span className="peer-badge">{peer.state}</span>
                </div>
                <div className="peer-row-grid">
                  <span>Direction</span>
                  <strong>{peer.outbound ? "Outbound" : "Inbound"}</strong>
                  <span>Height</span>
                  <strong>{peer.height}</strong>
                  <span>Height age</span>
                  <strong>{formatPeerAge(peer.height_age_secs)}</strong>
                </div>
              </article>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
