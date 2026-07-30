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
import { applyDesktopEvent } from "./desktopReducer";

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

const initialDesktopState: DesktopState = {
  mockMode: true,
  snapshot: null,
  process: null,
  message: "Ready",
  loading: false,
  error: null,
  wizardOpen: false,
  config: emptyConfig,
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
  const [state, dispatch] = React.useReducer(applyDesktopEvent, initialDesktopState);

  const refresh = React.useCallback(async () => {
    try {
      dispatch({ type: "DashboardRefreshStarted" });
      const snapshot = state.mockMode ? await getMockDashboardSnapshot() : await getDashboardSnapshot();
      dispatch({
        type: "DashboardSnapshotUpdated",
        snapshot,
        message: state.mockMode ? "Showing development mock data" : "Dashboard refreshed",
      });
      if (!state.mockMode) {
        dispatch({ type: "CoreProcessUpdated", process: await getCoreProcessState() });
      }
      dispatch({ type: "DesktopUpdateSettled" });
    } catch (err) {
      dispatch({ type: "DesktopActionFailed", message: String(err) });
    }
  }, [state.mockMode]);

  usePollingEffect(refresh, 5000);

  const runAction = React.useCallback(
    async (name: string, fn: () => Promise<unknown>) => {
      try {
        dispatch({ type: "DesktopActionStarted", name });
        await fn();
        await refresh();
        dispatch({ type: "DesktopActionCompleted", name });
      } catch (err) {
        dispatch({ type: "DesktopActionFailed", message: String(err) });
      }
    },
    [refresh],
  );

  const actions = React.useMemo<DesktopActions>(
    () => ({
      setMockMode: (mockMode) => dispatch({ type: "MockModeChanged", mockMode }),
      setWizardOpen: (open) => dispatch({ type: "WizardOpenChanged", open }),
      setConfig: (config) => dispatch({ type: "NodeConfigChanged", config }),
      refresh,
      startCore: () => runAction("Start", startCore),
      stopCore: () => runAction("Stop", stopCore),
      restartCore: () => runAction("Restart", restartCore),
      generateSupportPackage: () => runAction("Generate support package", generateSupportPackage),
      openLogsDirectory: () => runAction("Open logs", openLogsDirectory),
      openDataDirectory: () => runAction("Open data", openDataDirectory),
      saveNodeConfig: () => runAction("Save node config", () => saveNodeConfigService(state.config)),
    }),
    [refresh, runAction, state.config],
  );

  return { state, actions };
}
