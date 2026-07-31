import type { DesktopEvent } from "../events/desktopEvents";
import type { DesktopState } from "./desktopState";

export function applyDesktopEvent(state: DesktopState, event: DesktopEvent): DesktopState {
  switch (event.type) {
    case "ActiveViewChanged":
      return { ...state, activeView: event.view };
    case "MockModeChanged":
      return { ...state, mockMode: event.mockMode };
    case "WizardOpenChanged":
      return { ...state, wizardOpen: event.open };
    case "NodeConfigChanged":
      return { ...state, config: event.config };
    case "ExplorerModeChanged":
      return {
        ...state,
        explorer: {
          mode: event.mode,
          query: "",
          result: null,
          loading: false,
          error: null,
        },
      };
    case "ExplorerQueryChanged":
      return {
        ...state,
        explorer: { ...state.explorer, query: event.query },
      };
    case "ExplorerLookupStarted":
      return {
        ...state,
        message: event.message,
        explorer: { ...state.explorer, loading: true, error: null },
      };
    case "ExplorerResultUpdated":
      return {
        ...state,
        message: event.message,
        explorer: {
          ...state.explorer,
          loading: false,
          error: null,
          result: event.result,
        },
      };
    case "ExplorerLookupCleared":
      return {
        ...state,
        explorer: {
          ...state.explorer,
          result: null,
          error: null,
          loading: false,
        },
      };
    case "ExplorerLookupFailed":
      return {
        ...state,
        message: event.message,
        explorer: {
          ...state.explorer,
          loading: false,
          error: event.message,
        },
      };
    case "DashboardRefreshStarted":
      return { ...state, loading: true, error: null };
    case "DashboardSnapshotUpdated":
      return {
        ...state,
        snapshot: event.snapshot,
        message: event.message,
        lastUpdatedAt: event.receivedAt,
      };
    case "DiagnosticsUpdated":
      return { ...state, diagnostics: event.diagnostics };
    case "ConfigurationUpdated":
      return { ...state, configuration: event.configuration };
    case "WalletAccountUpdated":
      return { ...state, wallet: event.wallet };
    case "CoreProcessUpdated":
      return { ...state, process: event.process };
    case "DesktopUpdateSettled":
      return { ...state, loading: false };
    case "DesktopActionStarted":
      return { ...state, loading: true, error: null, message: `${event.name}...` };
    case "DesktopActionCompleted":
      return { ...state, loading: false, message: `${event.name} complete` };
    case "DesktopActionFailed":
      return { ...state, loading: false, error: event.message, message: event.message };
  }
}
