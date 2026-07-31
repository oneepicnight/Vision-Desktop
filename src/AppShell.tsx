import { Sidebar } from "./components/Sidebar";
import { StatusLine } from "./components/StatusLine";
import { DashboardGrid } from "./features/dashboard/DashboardGrid";
import { ExplorerPanel } from "./features/explorer/ExplorerPanel";
import { MiningPanel } from "./features/mining/MiningPanel";
import { CreateNodeWizard } from "./features/node-manager/CreateNodeWizard";
import { NodeControls } from "./features/node-manager/NodeControls";
import { PeerManagerPanel } from "./features/peers/PeerManagerPanel";
import { useDesktopState } from "./state/desktopState";

export function AppShell() {
  const { state, actions } = useDesktopState();

  const title =
    state.activeView === "explorer"
      ? "Blockchain Explorer"
      : state.activeView === "peers"
        ? "Peer Manager"
        : state.activeView === "mining"
          ? "Mining Status"
          : "Node Manager";

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
