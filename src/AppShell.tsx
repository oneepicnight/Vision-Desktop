import { Sidebar } from "./components/Sidebar";
import { StatusLine } from "./components/StatusLine";
import { DashboardGrid } from "./features/dashboard/DashboardGrid";
import { ExplorerPanel } from "./features/explorer/ExplorerPanel";
import { CreateNodeWizard } from "./features/node-manager/CreateNodeWizard";
import { NodeControls } from "./features/node-manager/NodeControls";
import { useDesktopState } from "./state/desktopState";

export function AppShell() {
  const { state, actions } = useDesktopState();
  const isExplorerView = state.activeView === "explorer";

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
            <h1>{isExplorerView ? "Blockchain Explorer" : "Node Manager"}</h1>
            <p>{state.snapshot?.mock_mode ? "Development mock mode" : "Real Core mode"}</p>
          </div>
          <NodeControls state={state} actions={actions} />
        </header>

        <StatusLine message={state.message} />

        {isExplorerView ? (
          <ExplorerPanel state={state} actions={actions} />
        ) : (
          <DashboardGrid state={state} actions={actions} />
        )}

        {!isExplorerView ? (
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
