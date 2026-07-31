import { Sidebar } from "./components/Sidebar";
import { StatusLine } from "./components/StatusLine";
import { ConfigurationPanel } from "./features/configuration/ConfigurationPanel";
import { DashboardGrid } from "./features/dashboard/DashboardGrid";
import { DiagnosticsPanel } from "./features/diagnostics/DiagnosticsPanel";
import { ExplorerPanel } from "./features/explorer/ExplorerPanel";
import { MiningPanel } from "./features/mining/MiningPanel";
import { CreateNodeWizard } from "./features/node-manager/CreateNodeWizard";
import { NodeControls } from "./features/node-manager/NodeControls";
import { PeerManagerPanel } from "./features/peers/PeerManagerPanel";
import { WalletPanel } from "./features/wallet/WalletPanel";
import { useDesktopState } from "./state/desktopState";

const viewTitles = {
  dashboard: "Node Manager",
  wallet: "Wallet",
  explorer: "Blockchain Explorer",
  peers: "Peer Manager",
  mining: "Mining Status",
  diagnostics: "Diagnostics",
  configuration: "Node Configuration",
} as const;

export function AppShell() {
  const { state, actions } = useDesktopState();
  const title = viewTitles[state.activeView];

  return (
    <main className="app-shell">
      <Sidebar
        activeView={state.activeView}
        onViewChange={actions.setActiveView}
        mockMode={state.mockMode}
        onMockModeChange={actions.setMockMode}
      />

      <section className="content">
        <header className="topbar">
          <div>
            <h1>{title}</h1>
            <p>{state.snapshot?.mock_mode ? "Development mock mode" : "Real Core mode"}</p>
          </div>
          <NodeControls state={state} actions={actions} />
        </header>

        <StatusLine message={state.message} />

        {state.activeView === "explorer" ? (
          <ExplorerPanel state={state} actions={actions} />
        ) : state.activeView === "peers" ? (
          <PeerManagerPanel state={state} actions={actions} />
        ) : state.activeView === "mining" ? (
          <MiningPanel state={state} />
        ) : state.activeView === "diagnostics" ? (
          <DiagnosticsPanel state={state} actions={actions} />
        ) : state.activeView === "wallet" ? (
          <WalletPanel state={state} />
        ) : state.activeView === "configuration" ? (
          <ConfigurationPanel state={state} />
        ) : (
          <DashboardGrid state={state} actions={actions} />
        )}

        {state.activeView === "dashboard" ? (
          <CreateNodeWizard
            wizardOpen={state.wizardOpen}
            config={state.config}
            onWizardOpenChange={actions.setWizardOpen}
            onConfigChange={actions.setConfig}
            actions={actions}
          />
        ) : null}
      </section>
    </main>
  );
}