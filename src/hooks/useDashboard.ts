import React from "react";
import { getCoreProcessState, getDashboardSnapshot, getMockDashboardSnapshot } from "../services/coreApi";
import type { DashboardSnapshot, ProcessState } from "../types/core";

export function useDashboard(mockMode: boolean) {
  const [snapshot, setSnapshot] = React.useState<DashboardSnapshot | null>(null);
  const [process, setProcess] = React.useState<ProcessState | null>(null);
  const [message, setMessage] = React.useState("Ready");

  const refresh = React.useCallback(async () => {
    try {
      const data = mockMode ? await getMockDashboardSnapshot() : await getDashboardSnapshot();
      setSnapshot(data);
      if (!mockMode) setProcess(await getCoreProcessState());
      setMessage(mockMode ? "Showing development mock data" : "Dashboard refreshed");
    } catch (err) {
      setMessage(String(err));
    }
  }, [mockMode]);

  React.useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, 5000);
    return () => window.clearInterval(id);
  }, [refresh]);

  const action = React.useCallback(
    async (name: string, fn: () => Promise<unknown>) => {
      try {
        setMessage(`${name}...`);
        await fn();
        await refresh();
        setMessage(`${name} complete`);
      } catch (err) {
        setMessage(String(err));
      }
    },
    [refresh],
  );

  return {
    snapshot,
    process,
    message,
    refresh,
    action,
  };
}
