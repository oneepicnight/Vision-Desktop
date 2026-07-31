import React from "react";
import { usePollingEffect } from "../hooks/usePollingEffect";
import {
  generateSupportPackage,
  getCoreProcessState,
  getDashboardSnapshot,
  getMockDashboardSnapshot,
  lookupExplorerAddress,
  lookupExplorerTransaction,
  openDataDirectory,
  openLogsDirectory,
  restartCore,
  saveNodeConfig as saveNodeConfigService,
  searchMockExplorer,
  startCore,
  stopCore,
} from "../services/coreApi";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";
import type {
  DesktopView,
  ExplorerLookupMode,
  ExplorerResult,
  ExplorerState,
} from "../types/explorer";
import { applyDesktopEvent } from "./desktopReducer";
import {
  beginDesktopRequest,
  createDesktopRequestTracker,
  invalidateDesktopRequestsForModeChange,
  isDesktopRequestCurrent,
  type DesktopRequestToken,
} from "./desktopRequestTracker";

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

const initialExplorerState: ExplorerState = {
  mode: "address",
  query: "",
  result: null,
  loading: false,
  error: null,
};

const initialDesktopState: DesktopState = {
  activeView: "dashboard",
  mockMode: true,
  snapshot: null,
  process: null,
  message: "Ready",
  loading: false,
  error: null,
  wizardOpen: false,
  config: emptyConfig,
  explorer: initialExplorerState,
  lastUpdatedAt: null,
};

export type DesktopState = {
  activeView: DesktopView;
  mockMode: boolean;
  snapshot: DashboardSnapshot | null;
  process: ProcessState | null;
  message: string;
  loading: boolean;
  error: string | null;
  wizardOpen: boolean;
  config: NodeConfig;
  explorer: ExplorerState;
  lastUpdatedAt: number | null;
};

export type DesktopActions = {
  setActiveView: (view: DesktopView) => void;
  setMockMode: (mockMode: boolean) => void;
  setWizardOpen: (open: boolean) => void;
  setConfig: (config: NodeConfig) => void;
  setExplorerMode: (mode: ExplorerLookupMode) => void;
  setExplorerQuery: (query: string) => void;
  searchExplorer: () => Promise<void>;
  clearExplorerResult: () => void;
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
  const requestTrackerRef = React.useRef(createDesktopRequestTracker());
  const explorerRequestTrackerRef = React.useRef(createDesktopRequestTracker());

  const refresh = React.useCallback(
    async (existingToken?: DesktopRequestToken) => {
      const token = existingToken ?? beginDesktopRequest(requestTrackerRef.current);
      const mockMode = state.mockMode;

      try {
        dispatch({ type: "DashboardRefreshStarted" });
        const snapshot = mockMode
          ? await getMockDashboardSnapshot()
          : await getDashboardSnapshot();
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({
          type: "DashboardSnapshotUpdated",
          snapshot,
          message: mockMode
            ? "Showing development mock data"
            : "Dashboard refreshed",
          receivedAt: Date.now(),
        });
        if (!mockMode) {
          const process = await getCoreProcessState();
          if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
            return;
          }
          dispatch({ type: "CoreProcessUpdated", process });
        }
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({ type: "DesktopUpdateSettled" });
      } catch (err) {
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({ type: "DesktopActionFailed", message: String(err) });
      }
    },
    [state.mockMode],
  );

  usePollingEffect(refresh, 5000);

  const runAction = React.useCallback(
    async (name: string, fn: () => Promise<unknown>) => {
      const token = beginDesktopRequest(requestTrackerRef.current);

      try {
        dispatch({ type: "DesktopActionStarted", name });
        await fn();
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        await refresh(token);
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({ type: "DesktopActionCompleted", name });
      } catch (err) {
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({ type: "DesktopActionFailed", message: String(err) });
      }
    },
    [refresh],
  );

  const searchExplorer = React.useCallback(async () => {
    const query = state.explorer.query.trim();
    if (query.length === 0) {
      dispatch({
        type: "ExplorerLookupFailed",
        message:
          state.explorer.mode === "address"
            ? "Enter an address before running the explorer lookup"
            : "Enter a transaction ID before running the explorer lookup",
      });
      return;
    }

    const token = beginDesktopRequest(explorerRequestTrackerRef.current);
    const mode = state.explorer.mode;
    const mockMode = state.mockMode;
    dispatch({
      type: "ExplorerLookupStarted",
      message:
        mode === "address" ? "Looking up address..." : "Looking up transaction...",
    });

    try {
      const result: ExplorerResult =
        mode === "address"
          ? mockMode
            ? await searchMockExplorer("address", query)
            : await lookupExplorerAddress(query)
          : mockMode
            ? await searchMockExplorer("transaction", query)
            : await lookupExplorerTransaction(query);

      if (!isDesktopRequestCurrent(explorerRequestTrackerRef.current, token)) {
        return;
      }

      dispatch({
        type: "ExplorerResultUpdated",
        result,
        message:
          mode === "address"
            ? "Address lookup complete"
            : "Transaction lookup complete",
      });
    } catch (err) {
      if (!isDesktopRequestCurrent(explorerRequestTrackerRef.current, token)) {
        return;
      }
      dispatch({ type: "ExplorerLookupFailed", message: String(err) });
    }
  }, [state.explorer.mode, state.explorer.query, state.mockMode]);

  const actions = React.useMemo<DesktopActions>(
    () => ({
      setActiveView: (view) => dispatch({ type: "ActiveViewChanged", view }),
      setMockMode: (mockMode) => {
        invalidateDesktopRequestsForModeChange(requestTrackerRef.current);
        invalidateDesktopRequestsForModeChange(explorerRequestTrackerRef.current);
        dispatch({ type: "MockModeChanged", mockMode });
      },
      setWizardOpen: (open) => dispatch({ type: "WizardOpenChanged", open }),
      setConfig: (config) => dispatch({ type: "NodeConfigChanged", config }),
      setExplorerMode: (mode) => {
        invalidateDesktopRequestsForModeChange(explorerRequestTrackerRef.current);
        dispatch({ type: "ExplorerModeChanged", mode });
      },
      setExplorerQuery: (query) => dispatch({ type: "ExplorerQueryChanged", query }),
      searchExplorer,
      clearExplorerResult: () => dispatch({ type: "ExplorerLookupCleared" }),
      refresh: () => refresh(),
      startCore: () => runAction("Start", startCore),
      stopCore: () => runAction("Stop", stopCore),
      restartCore: () => runAction("Restart", restartCore),
      generateSupportPackage: () =>
        runAction("Generate support package", generateSupportPackage),
      openLogsDirectory: () => runAction("Open logs", openLogsDirectory),
      openDataDirectory: () => runAction("Open data", openDataDirectory),
      saveNodeConfig: () =>
        runAction("Save node config", () => saveNodeConfigService(state.config)),
    }),
    [refresh, runAction, searchExplorer, state.config],
  );

  return { state, actions };
}
