import type { DesktopEvent } from "../events/desktopEvents";
import type { DesktopState } from "./desktopState";

export function applyDesktopEvent(state: DesktopState, event: DesktopEvent): DesktopState {
  switch (event.type) {
    case "MockModeChanged":
      return { ...state, mockMode: event.mockMode };
    case "WizardOpenChanged":
      return { ...state, wizardOpen: event.open };
    case "NodeConfigChanged":
      return { ...state, config: event.config };
    case "DashboardRefreshStarted":
      return { ...state, loading: true, error: null };
    case "DashboardSnapshotUpdated":
      return { ...state, snapshot: event.snapshot, message: event.message };
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
