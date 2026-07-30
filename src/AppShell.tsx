import { Sidebar } from "./components/Sidebar";
import { StatusLine } from "./components/StatusLine";
import { DashboardGrid } from "./features/dashboard/DashboardGrid";
import { CreateNodeWizard } from "./features/node-manager/CreateNodeWizard";
import { NodeControls } from "./features/node-manager/NodeControls";
import { useDesktopState } from "./state/desktopState";

export function AppShell() {
  const { state, actions } = useDesktopState();

  return (
    <main className="app-shell">
      <Sidebar mockMode={state.mockMode} onMockModeChange={actions.setMockMode} />

      <section className="content">
        <header className="topbar">
          <div>
            <h1>Node Manager</h1>
            <p>{state.snapshot?.mock_mode ? "Development mock mode" : "Real Core mode"}</p>
          </div>
          <NodeControls state={state} actions={actions} />
        </header>

        <StatusLine message={state.message} />

        <DashboardGrid state={state} actions={actions} />

        <CreateNodeWizard
          wizardOpen={state.wizardOpen}
          config={state.config}
          onWizardOpenChange={actions.setWizardOpen}
          onConfigChange={actions.setConfig}
          actions={actions}
        />
      </section>
    </main>
  );
}
