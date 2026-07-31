import type { DesktopState } from "../../state/desktopState";

export type LifecycleActionKind = "start" | "stop" | "restart";

export type LifecycleControl = {
  enabled: boolean;
  label: string;
  reason: string;
};

export type LifecycleControlsViewModel = {
  processState: string;
  recoveryState: string;
  mockMode: boolean;
  actionInFlight: LifecycleActionKind | null;
  pendingConfirmation: LifecycleActionKind | null;
  start: LifecycleControl;
  stop: LifecycleControl;
  restart: LifecycleControl;
  refreshEnabled: boolean;
  refreshReason: string;
  progressMessage: string | null;
  recoveryNote: string | null;
  confirmationTitle: string | null;
  confirmationBody: string | null;
};

function makeControl(enabled: boolean, label: string, reason: string): LifecycleControl {
  return { enabled, label, reason };
}

function lifecycleProcessState(state: DesktopState) {
  return state.snapshot?.process_state ?? state.process?.state ?? "Unknown";
}

function recoveryState(state: DesktopState) {
  return state.snapshot?.status?.recovery?.state ?? "Unknown";
}

export function canStartCore(state: DesktopState) {
  if (state.mockMode) return false;
  if (state.activeLifecycleAction != null) return false;
  if (state.pendingLifecycleConfirmation != null) return false;
  const processState = lifecycleProcessState(state);
  return processState === "stopped" || processState === "crashed";
}

export function canStopCore(state: DesktopState) {
  if (state.mockMode) return false;
  if (state.activeLifecycleAction != null) return false;
  if (state.pendingLifecycleConfirmation != null) return false;
  const processState = lifecycleProcessState(state);
  return processState === "running" || processState === "crashed";
}

export function canRestartCore(state: DesktopState) {
  if (state.mockMode) return false;
  if (state.activeLifecycleAction != null) return false;
  if (state.pendingLifecycleConfirmation != null) return false;
  const processState = lifecycleProcessState(state);
  return processState === "running" || processState === "crashed";
}

export function deriveLifecycleControls(state: DesktopState): LifecycleControlsViewModel {
  const processState = lifecycleProcessState(state);
  const recovery = recoveryState(state);
  const actionInFlight = state.activeLifecycleAction;
  const pendingConfirmation = state.pendingLifecycleConfirmation;

  const progressMessage =
    actionInFlight == null
      ? null
      : actionInFlight === "start"
        ? "Start command in progress. Vision Desktop will rely on refreshed process state before implying that Core is running."
        : actionInFlight === "stop"
          ? "Stop command in progress. Vision Desktop will rely on refreshed process state before implying that Core is stopped."
          : "Restart command in progress. Vision Desktop will rely on refreshed process state after the command completes.";

  const recoveryNote =
    recovery !== "Unknown" && recovery !== "normal"
      ? `Core currently reports recovery state ${recovery}. Lifecycle controls are not disabled by recovery state alone.`
      : null;

  let startReason = "Start is available.";
  let stopReason = "Stop is available.";
  let restartReason = "Restart is available.";

  if (state.mockMode) {
    startReason = "Lifecycle controls are disabled in mock mode.";
    stopReason = startReason;
    restartReason = startReason;
  } else if (pendingConfirmation != null) {
    const message = "Restart confirmation is pending.";
    startReason = message;
    stopReason = message;
    restartReason = message;
  } else if (actionInFlight != null) {
    const message = `${actionInFlight[0].toUpperCase()}${actionInFlight.slice(1)} is already in progress.`;
    startReason = message;
    stopReason = message;
    restartReason = message;
  } else {
    if (!canStartCore(state)) {
      startReason =
        processState === "running"
          ? "Core is already running."
          : processState === "Unknown"
            ? "Current process state is unknown."
            : "Start is only available when Core is stopped or crashed.";
    }
    if (!canStopCore(state)) {
      stopReason =
        processState === "stopped"
          ? "Core is already stopped."
          : processState === "Unknown"
            ? "Current process state is unknown."
            : "Stop is only available when Core is running or crashed.";
    }
    if (!canRestartCore(state)) {
      restartReason =
        processState === "stopped"
          ? "Restart is unavailable while Core is stopped."
          : processState === "Unknown"
            ? "Current process state is unknown."
            : "Restart is only available when Core is running or crashed.";
    }
  }

  return {
    processState,
    recoveryState: recovery,
    mockMode: state.mockMode,
    actionInFlight,
    pendingConfirmation,
    start: makeControl(
      canStartCore(state),
      actionInFlight === "start" ? "Starting…" : "Start",
      startReason,
    ),
    stop: makeControl(
      canStopCore(state),
      actionInFlight === "stop" ? "Stopping…" : "Stop",
      stopReason,
    ),
    restart: makeControl(
      canRestartCore(state),
      actionInFlight === "restart" ? "Restarting…" : "Restart",
      restartReason,
    ),
    refreshEnabled: actionInFlight == null,
    refreshReason:
      actionInFlight == null
        ? "Refresh is available."
        : "Refresh is temporarily disabled while a lifecycle action is in progress.",
    progressMessage,
    recoveryNote,
    confirmationTitle:
      pendingConfirmation === "restart" ? "Confirm restart" : null,
    confirmationBody:
      pendingConfirmation === "restart"
        ? "The node process will be stopped and started again. This action is cancelable until you confirm it."
        : null,
  };
}