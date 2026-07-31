import { Activity, Network, Search, Settings, Terminal, Wifi } from "lucide-react";
import type { DesktopView } from "../types/explorer";

type SidebarProps = {
  activeView: DesktopView;
  onViewChange: (view: DesktopView) => void;
  mockMode: boolean;
  onMockModeChange: (mockMode: boolean) => void;
};

export function Sidebar({
  activeView,
  onViewChange,
  mockMode,
  onMockModeChange,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="mark">V</div>
        <div>
          <h1>Vision</h1>
          <p>Desktop 0.1.0 alpha</p>
        </div>
      </div>
      <button
        className={`nav ${activeView === "dashboard" ? "active" : ""}`}
        onClick={() => onViewChange("dashboard")}
      >
        <Activity size={18} />Dashboard
      </button>
      <button
        className={`nav ${activeView === "explorer" ? "active" : ""}`}
        onClick={() => onViewChange("explorer")}
      >
        <Search size={18} />Explorer
      </button>
      <button
        className={`nav ${activeView === "peers" ? "active" : ""}`}
        onClick={() => onViewChange("peers")}
      >
        <Network size={18} />Peers
      </button>
      <button className="nav">
        <Wifi size={18} />Networking
      </button>
      <button className="nav">
        <Terminal size={18} />Logs
      </button>
      <button className="nav">
        <Settings size={18} />Settings
      </button>
      <label className="toggle">
        <input
          type="checkbox"
          checked={mockMode}
          onChange={(event) => onMockModeChange(event.target.checked)}
        />
        Mock mode
      </label>
    </aside>
  );
}
