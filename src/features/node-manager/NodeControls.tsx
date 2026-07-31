import { AlertTriangle, Play, RefreshCw, RotateCw, Square } from "lucide-react";
import type { DesktopActions, DesktopState } from "../../state/desktopState";
import { deriveLifecycleControls } from "./lifecycleControls";

type NodeControlsProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function NodeControls({ state, actions }: NodeControlsProps) {
  const controls = deriveLifecycleControls(state);

  return (
    <div className="node-controls-stack">
      <div className="actions">
        <button
          onClick={() => actions.startCore()}
          disabled={!controls.start.enabled}
          title={controls.start.reason}
        >
          <Play size={18} />{controls.start.label}
        </button>
        <button
          onClick={() => actions.stopCore()}
          disabled={!controls.stop.enabled}
          title={controls.stop.reason}
        >
          <Square size={18} />{controls.stop.label}
        </button>
        <button
          onClick={() => actions.restartCore()}
          disabled={!controls.restart.enabled}
          title={controls.restart.reason}
        >
          <RotateCw size={18} />{controls.restart.label}
        </button>
        <button
          onClick={() => actions.refresh()}
          disabled={!controls.refreshEnabled}
          title={controls.refreshReason}
        >
          <RefreshCw size={18} />Refresh
        </button>
      </div>

      {controls.progressMessage ? (
        <p className="lifecycle-note">{controls.progressMessage}</p>
      ) : null}

      {controls.recoveryNote ? (
        <p className="lifecycle-note">{controls.recoveryNote}</p>
      ) : null}

      {controls.pendingConfirmation === "restart" ? (
        <div className="lifecycle-confirmation" role="alert">
          <div className="lifecycle-confirmation-title">
            <AlertTriangle size={18} />
            <strong>{controls.confirmationTitle}</strong>
          </div>
          <p>{controls.confirmationBody}</p>
          <div className="button-stack lifecycle-confirmation-actions">
            <button onClick={() => actions.confirmRestartCore()} disabled={state.activeLifecycleAction != null}>
              Confirm restart
            </button>
            <button onClick={actions.cancelLifecycleConfirmation} disabled={state.activeLifecycleAction != null}>
              Cancel
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}