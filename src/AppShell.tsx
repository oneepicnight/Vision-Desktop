import React from "react";
import { Sidebar } from "./components/Sidebar";
import { StatusLine } from "./components/StatusLine";
import { DashboardGrid } from "./features/dashboard/DashboardGrid";
import { CreateNodeWizard, emptyConfig } from "./features/node-manager/CreateNodeWizard";
import { NodeControls } from "./features/node-manager/NodeControls";
import { useDashboard } from "./hooks/useDashboard";
import type { NodeConfig } from "./types/core";

export function AppShell() {
  const [mockMode, setMockMode] = React.useState(true);
  const [wizardOpen, setWizardOpen] = React.useState(false);
  const [config, setConfig] = React.useState<NodeConfig>(emptyConfig);
  const { snapshot, process, message, refresh, action } = useDashboard(mockMode);

  return (
    <main className="app-shell">
      <Sidebar mockMode={mockMode} onMockModeChange={setMockMode} />

      <section className="content">
        <header className="topbar">
          <div>
            <h1>Node Manager</h1>
            <p>{snapshot?.mock_mode ? "Development mock mode" : "Real Core mode"}</p>
          </div>
          <NodeControls mockMode={mockMode} refresh={refresh} action={action} />
        </header>

        <StatusLine message={message} />

        <DashboardGrid snapshot={snapshot} process={process} mockMode={mockMode} action={action} />

        <CreateNodeWizard
          wizardOpen={wizardOpen}
          config={config}
          onWizardOpenChange={setWizardOpen}
          onConfigChange={setConfig}
          action={action}
        />
      </section>
    </main>
  );
}
