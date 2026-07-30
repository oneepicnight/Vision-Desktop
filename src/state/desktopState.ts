import React from "react";
import { usePollingEffect } from "../hooks/usePollingEffect";
import {
  generateSupportPackage,
  getCoreProcessState,
  getDashboardSnapshot,
  getMockDashboardSnapshot,
  openDataDirectory,
  openLogsDirectory,
  restartCore,
  saveNodeConfig as saveNodeConfigService,
  startCore,
  stopCore,
} from "../services/coreApi";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";

const emptyConfig: NodeConfig = {
  node_name: "Default Node",
  mode: "LocalTesting",
  api_port: 0,
  p2p_port: 19090,
  seed_peers: [],
  advertised_host: null,
  advertised_port: null,
  mining_enabled: false,
  miner_reward_address: "0000000000000000000000000000000000000000000000000000000000000000",
  data_dir: "",
  log_dir: "",
};

export type DesktopState = {
  mockMode: boolean;
  snapshot: DashboardSnapshot | null;
  process: ProcessState | null;
  message: string;
  loading: boolean;
  error: string | null;
  wizardOpen: boolean;
  config: NodeConfig;
};

export type DesktopActions = {
  setMockMode: (mockMode: boolean) => void;
  setWizardOpen: (open: boolean) => void;
  setConfig: (config: NodeConfig) => void;
  refresh: () => Promise<void>;
  startCore: () => Promise<void>;
  stopCore: () => Promise<void>;
  restartCore: () => Promise<void>;
  generateSupportPackage: () => Promise<void>;
  openLogsDirectory: () => Promise<void>;
  openDataDirectory: () => Promise<void>;
  saveNodeConfig: () => Promise<void>;
};

export type DesktopStateController = {
  state: DesktopState;
  actions: DesktopActions;
};

export function useDesktopState(): DesktopStateController {
  const [mockMode, setMockMode] = React.useState(true);
  const [snapshot, setSnapshot] = React.useState<DashboardSnapshot | null>(null);
  const [process, setProcess] = React.useState<ProcessState | null>(null);
  const [message, setMessage] = React.useState("Ready");
  const [loading, setLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [wizardOpen, setWizardOpen] = React.useState(false);
  const [config, setConfig] = React.useState<NodeConfig>(emptyConfig);

  const refresh = React.useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const data = mockMode ? await getMockDashboardSnapshot() : await getDashboardSnapshot();
      setSnapshot(data);
      if (!mockMode) setProcess(await getCoreProcessState());
      setMessage(mockMode ? "Showing development mock data" : "Dashboard refreshed");
    } catch (err) {
      const errorMessage = String(err);
      setError(errorMessage);
      setMessage(errorMessage);
    } finally {
      setLoading(false);
    }
  }, [mockMode]);

  usePollingEffect(refresh, 5000);

  const runAction = React.useCallback(
    async (name: string, fn: () => Promise<unknown>) => {
      try {
        setLoading(true);
        setError(null);
        setMessage(`${name}...`);
        await fn();
        await refresh();
        setMessage(`${name} complete`);
      } catch (err) {
        const errorMessage = String(err);
        setError(errorMessage);
        setMessage(errorMessage);
      } finally {
        setLoading(false);
      }
    },
    [refresh],
  );

  const actions = React.useMemo<DesktopActions>(
    () => ({
      setMockMode,
      setWizardOpen,
      setConfig,
      refresh,
      startCore: () => runAction("Start", startCore),
      stopCore: () => runAction("Stop", stopCore),
      restartCore: () => runAction("Restart", restartCore),
      generateSupportPackage: () => runAction("Generate support package", generateSupportPackage),
      openLogsDirectory: () => runAction("Open logs", openLogsDirectory),
      openDataDirectory: () => runAction("Open data", openDataDirectory),
      saveNodeConfig: () => runAction("Save node config", () => saveNodeConfigService(config)),
    }),
    [config, refresh, runAction],
  );

  return {
    state: {
      mockMode,
      snapshot,
      process,
      message,
      loading,
      error,
      wizardOpen,
      config,
    },
    actions,
  };
}
