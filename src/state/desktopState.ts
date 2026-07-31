import React from "react";
import { deriveLifecycleControls } from "../features/node-manager/lifecycleControls";
import { usePollingEffect } from "../hooks/usePollingEffect";
import {
  generateSupportPackage,
  getCoreManifest,
  getCoreProcessState,
  getCoreStderrTail,
  getCoreStdoutTail,
  getDashboardSnapshot,
  getDefaultPaths,
  getMockDashboardSnapshot,
  getNodeConfigSnapshot,
  lookupExplorerAddress,
  lookupExplorerTransaction,
  openDataDirectory,
  openLogsDirectory,
  restartCore,
  saveNodeConfig as saveNodeConfigService,
  searchMockExplorer,
  startCore,
  stopCore,
  verifyCoreBinary,
} from "../services/coreApi";
import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";
import type { ConfigurationState } from "../types/configuration";
import type { DiagnosticsState } from "../types/diagnostics";
import type {
  DesktopView,
  ExplorerAddressResult,
  ExplorerLookupMode,
  ExplorerResult,
  ExplorerState,
} from "../types/explorer";
import type { WalletAccountState } from "../types/wallet";
import { applyDesktopEvent } from "./desktopReducer";
import {
  beginDesktopRequest,
  createDesktopRequestTracker,
  invalidateDesktopRequestsForModeChange,
  isDesktopRequestCurrent,
  type DesktopRequestToken,
} from "./desktopRequestTracker";
import type { LifecycleActionKind } from "../features/node-manager/lifecycleControls";

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

const initialDiagnosticsState: DiagnosticsState = {
  manifest: null,
  verification: null,
  stdoutTail: null,
  stderrTail: null,
  error: null,
};

const initialWalletState: WalletAccountState = {
  queriedAddress: null,
  account: null,
  error: null,
};

const initialConfigurationState: ConfigurationState = {
  snapshot: null,
  appPaths: null,
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
  diagnostics: initialDiagnosticsState,
  configuration: initialConfigurationState,
  wallet: initialWalletState,
  lastUpdatedAt: null,
  activeLifecycleAction: null,
  pendingLifecycleConfirmation: null,
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
  diagnostics: DiagnosticsState;
  configuration: ConfigurationState;
  wallet: WalletAccountState;
  lastUpdatedAt: number | null;
  activeLifecycleAction: LifecycleActionKind | null;
  pendingLifecycleConfirmation: LifecycleActionKind | null;
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
  confirmRestartCore: () => Promise<void>;
  cancelLifecycleConfirmation: () => void;
  generateSupportPackage: () => Promise<void>;
  openLogsDirectory: () => Promise<void>;
  openDataDirectory: () => Promise<void>;
  saveNodeConfig: () => Promise<void>;
};

export type DesktopStateController = {
  state: DesktopState;
  actions: DesktopActions;
};

function ensureAddressResult(result: ExplorerResult): ExplorerAddressResult {
  if (result.kind !== "address") {
    throw new Error("expected address lookup result");
  }
  return result;
}

function lifecycleStartMessage(action: LifecycleActionKind) {
  if (action === "start") {
    return "Start command requested...";
  }
  if (action === "stop") {
    return "Stop command requested...";
  }
  return "Restart command requested...";
}

function lifecycleCompletedMessage(action: LifecycleActionKind) {
  if (action === "start") {
    return "Start command completed; confirm the observed process state below.";
  }
  if (action === "stop") {
    return "Stop command completed; confirm the observed process state below.";
  }
  return "Restart command completed; confirm the observed process state below.";
}

export function useDesktopState(): DesktopStateController {
  const [state, dispatch] = React.useReducer(applyDesktopEvent, initialDesktopState);
  const requestTrackerRef = React.useRef(createDesktopRequestTracker());
  const explorerRequestTrackerRef = React.useRef(createDesktopRequestTracker());

  const refresh = React.useCallback(
    async (
      source: "manual" | "polling" | "action" = "manual",
      existingToken?: DesktopRequestToken,
    ) => {
      if (source === "polling" && state.activeLifecycleAction != null) {
        return;
      }

      const token = existingToken ?? beginDesktopRequest(requestTrackerRef.current);
      const mockMode = state.mockMode;
      const configuredRewardAddress = state.config.miner_reward_address.trim();

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

        if (state.activeView === "configuration") {
          const configurationResults = await Promise.allSettled([
            getNodeConfigSnapshot(),
            getDefaultPaths(),
          ]);
          if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
            return;
          }
          const configurationErrors = configurationResults
            .filter((result): result is PromiseRejectedResult => result.status === "rejected")
            .map((result) => String(result.reason));
          dispatch({
            type: "ConfigurationUpdated",
            configuration: {
              snapshot:
                configurationResults[0].status === "fulfilled"
                  ? configurationResults[0].value
                  : null,
              appPaths:
                configurationResults[1].status === "fulfilled"
                  ? configurationResults[1].value
                  : null,
              error: configurationErrors.length > 0 ? configurationErrors.join(" | ") : null,
            },
          });
        }

        if (state.activeView === "wallet" && mockMode) {
          if (configuredRewardAddress.length === 0) {
            dispatch({
              type: "WalletAccountUpdated",
              wallet: { queriedAddress: null, account: null, error: null },
            });
          } else {
            const mockWalletResult = ensureAddressResult(
              await searchMockExplorer("address", configuredRewardAddress),
            );
            if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
              return;
            }
            dispatch({
              type: "WalletAccountUpdated",
              wallet: {
                queriedAddress: configuredRewardAddress,
                account: mockWalletResult,
                error: null,
              },
            });
          }
        }

        if (!mockMode) {
          const process = await getCoreProcessState();
          if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
            return;
          }
          dispatch({ type: "CoreProcessUpdated", process });

          if (state.activeView === "diagnostics") {
            const diagnosticsResults = await Promise.allSettled([
              getCoreManifest(),
              verifyCoreBinary(),
              getCoreStdoutTail(),
              getCoreStderrTail(),
            ]);
            if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
              return;
            }

            const diagnosticsError = diagnosticsResults.find(
              (result): result is PromiseRejectedResult => result.status === "rejected",
            );

            dispatch({
              type: "DiagnosticsUpdated",
              diagnostics: {
                manifest:
                  diagnosticsResults[0].status === "fulfilled"
                    ? diagnosticsResults[0].value
                    : null,
                verification:
                  diagnosticsResults[1].status === "fulfilled"
                    ? diagnosticsResults[1].value
                    : null,
                stdoutTail:
                  diagnosticsResults[2].status === "fulfilled"
                    ? diagnosticsResults[2].value
                    : null,
                stderrTail:
                  diagnosticsResults[3].status === "fulfilled"
                    ? diagnosticsResults[3].value
                    : null,
                error: diagnosticsError ? String(diagnosticsError.reason) : null,
              },
            });
          }

          if (state.activeView === "wallet") {
            if (configuredRewardAddress.length === 0) {
              dispatch({
                type: "WalletAccountUpdated",
                wallet: { queriedAddress: null, account: null, error: null },
              });
            } else if (process.state !== "running" || snapshot.api_error) {
              dispatch({
                type: "WalletAccountUpdated",
                wallet: {
                  queriedAddress: configuredRewardAddress,
                  account: null,
                  error: null,
                },
              });
            } else {
              try {
                const walletAccount = await lookupExplorerAddress(configuredRewardAddress);
                if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
                  return;
                }
                dispatch({
                  type: "WalletAccountUpdated",
                  wallet: {
                    queriedAddress: configuredRewardAddress,
                    account: walletAccount,
                    error: null,
                  },
                });
              } catch (walletErr) {
                if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
                  return;
                }
                dispatch({
                  type: "WalletAccountUpdated",
                  wallet: {
                    queriedAddress: configuredRewardAddress,
                    account: null,
                    error: String(walletErr),
                  },
                });
              }
            }
          }
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
    [state.activeLifecycleAction, state.activeView, state.config.miner_reward_address, state.mockMode],
  );

  usePollingEffect(() => refresh("polling"), 5000);

  const runAction = React.useCallback(
    async (name: string, fn: () => Promise<unknown>) => {
      const token = beginDesktopRequest(requestTrackerRef.current);

      try {
        dispatch({ type: "DesktopActionStarted", name });
        await fn();
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        await refresh("action", token);
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

  const runLifecycleAction = React.useCallback(
    async (action: LifecycleActionKind, fn: () => Promise<unknown>) => {
      const derived = deriveLifecycleControls(state);
      const allowed =
        action === "start"
          ? derived.start.enabled
          : action === "stop"
            ? derived.stop.enabled
            : derived.restart.enabled;
      const reason =
        action === "start"
          ? derived.start.reason
          : action === "stop"
            ? derived.stop.reason
            : derived.restart.reason;

      if (!allowed) {
        dispatch({ type: "DesktopActionFailed", message: reason });
        return;
      }

      const token = beginDesktopRequest(requestTrackerRef.current);

      try {
        dispatch({
          type: "LifecycleActionStarted",
          action,
          message: lifecycleStartMessage(action),
        });
        await fn();
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        await refresh("action", token);
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({
          type: "LifecycleActionCompleted",
          action,
          message: lifecycleCompletedMessage(action),
        });
      } catch (err) {
        if (!isDesktopRequestCurrent(requestTrackerRef.current, token)) {
          return;
        }
        dispatch({
          type: "LifecycleActionFailed",
          action,
          message: String(err),
        });
      }
    },
    [refresh, state],
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
      refresh: () => {
        if (state.activeLifecycleAction != null) {
          return Promise.resolve();
        }
        return refresh("manual");
      },
      startCore: () => runLifecycleAction("start", startCore),
      stopCore: () => runLifecycleAction("stop", stopCore),
      restartCore: async () => {
        const controls = deriveLifecycleControls(state);
        if (!controls.restart.enabled) {
          dispatch({ type: "DesktopActionFailed", message: controls.restart.reason });
          return;
        }
        dispatch({ type: "LifecycleConfirmationRequested", action: "restart" });
      },
      confirmRestartCore: () => {
        if (state.pendingLifecycleConfirmation !== "restart") {
          return Promise.resolve();
        }
        return runLifecycleAction("restart", restartCore);
      },
      cancelLifecycleConfirmation: () =>
        dispatch({ type: "LifecycleConfirmationDismissed" }),
      generateSupportPackage: () =>
        runAction("Generate support package", generateSupportPackage),
      openLogsDirectory: () => runAction("Open logs", openLogsDirectory),
      openDataDirectory: () => runAction("Open data", openDataDirectory),
      saveNodeConfig: () =>
        runAction("Save node config", () => saveNodeConfigService(state.config)),
    }),
    [refresh, runAction, runLifecycleAction, searchExplorer, state],
  );

  return { state, actions };
}