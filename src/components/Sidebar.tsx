import {
  Activity,
  Flame,
  Landmark,
  Network,
  Search,
  Settings,
  ShieldCheck,
  Terminal,
  Wifi,
  type LucideIcon,
} from "lucide-react";
import type { DesktopView } from "../types/explorer";

type SidebarProps = {
  activeView: DesktopView;
  onViewChange: (view: DesktopView) => void;
  mockMode: boolean;
  onMockModeChange: (mockMode: boolean) => void;
};

const primaryNavigation = [
  { view: "dashboard", label: "Dashboard", icon: Activity },
  { view: "wallet", label: "Wallet", icon: Landmark },
  { view: "explorer", label: "Explorer", icon: Search },
  { view: "peers", label: "Peers", icon: Network },
  { view: "mining", label: "Mining", icon: Flame },
  { view: "diagnostics", label: "Diagnostics", icon: ShieldCheck },
  { view: "configuration", label: "Configuration", icon: Settings },
] satisfies ReadonlyArray<{ view: DesktopView; label: string; icon: LucideIcon }>;

export function Sidebar({
  activeView,
  onViewChange,
  mockMode,
  onMockModeChange,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="mark" aria-hidden="true">
          <span>V</span>
        </div>
        <div className="brand-copy">
          <span className="brand-name">Vision</span>
          <p>Network Desktop</p>
        </div>
      </div>

      <nav className="sidebar-navigation" aria-label="Vision Desktop">
        <span className="nav-section-label">Operate</span>
        {primaryNavigation.map(({ view, label, icon: Icon }) => (
          <button
            type="button"
            key={view}
            className={`nav ${activeView === view ? "active" : ""}`}
            onClick={() => onViewChange(view)}
            aria-current={activeView === view ? "page" : undefined}
          >
            <span className="nav-icon" aria-hidden="true">
              <Icon size={18} />
            </span>
            <span>{label}</span>
          </button>
        ))}

        <span className="nav-section-label nav-section-secondary">Utilities</span>
        <button type="button" className="nav nav-muted" disabled>
          <span className="nav-icon" aria-hidden="true">
            <Wifi size={18} />
          </span>
          <span>Networking</span>
          <span className="nav-badge">Soon</span>
        </button>
        <button type="button" className="nav nav-muted" disabled>
          <span className="nav-icon" aria-hidden="true">
            <Terminal size={18} />
          </span>
          <span>Logs</span>
          <span className="nav-badge">Soon</span>
        </button>
      </nav>

      <div className="sidebar-footer">
        <label className="toggle">
          <span>
            <strong>Mock network</strong>
            <small>Local development data</small>
          </span>
          <span className="toggle-control">
            <input
              type="checkbox"
              checked={mockMode}
              onChange={(event) => onMockModeChange(event.target.checked)}
            />
            <span className="toggle-track" aria-hidden="true" />
          </span>
        </label>
        <span className="build-label">Desktop 0.1.0 alpha</span>
      </div>
    </aside>
  );
}
