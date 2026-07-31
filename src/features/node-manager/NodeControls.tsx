import {
  AlertTriangle,
  CircleDot,
  Play,
  RefreshCw,
  RotateCw,
  Server,
  ShieldAlert,
  Square,
} from "lucide-react";
import type { DesktopActions, DesktopState } from "../../state/desktopState";
import { deriveLifecycleControls } from "./lifecycleControls";

type NodeControlsProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function NodeControls({ state, actions }: NodeControlsProps) {
  const controls = deriveLifecycleControls(state);

  return (
    <div className="node-operations-console">
      <div className="node-operations-context" aria-label="Node lifecycle context">
        <span>
          <Server size={13} aria-hidden="true" />
          Process <strong>{controls.processState}</strong>
        </span>
        <span>
          <ShieldAlert size={13} aria-hidden="true" />
          Recovery <strong>{controls.recoveryState}</strong>
        </span>
        <span className={controls.mockMode ? "is-mock" : "is-live"}>
          <CircleDot size={13} aria-hidden="true" />
          {controls.mockMode ? "Mock controls locked" : "Live controls"}
        </span>
      </div>

      <div className="node-operations-actions" role="group" aria-label="Node lifecycle controls">
        <button
          className="node-operation-button is-start"
          onClick={() => actions.startCore()}
          disabled={!controls.start.enabled}
          title={controls.start.reason}
        >
          <Play size={17} aria-hidden="true" />
          {controls.start.label}
        </button>
        <button
          className="node-operation-button is-stop"
          onClick={() => actions.stopCore()}
          disabled={!controls.stop.enabled}
          title={controls.stop.reason}
        >
          <Square size={17} aria-hidden="true" />
          {controls.stop.label}
        </button>
        <button
          className="node-operation-button is-restart"
          onClick={() => actions.restartCore()}
          disabled={!controls.restart.enabled}
          title={controls.restart.reason}
        >
          <RotateCw size={17} aria-hidden="true" />
          {controls.restart.label}
        </button>
        <button
          className="node-operation-button is-refresh"
          onClick={() => actions.refresh()}
          disabled={!controls.refreshEnabled}
          title={controls.refreshReason}
        >
          <RefreshCw size={17} aria-hidden="true" />
          Refresh
        </button>
      </div>

      {controls.progressMessage ? (
        <p className="node-operations-note is-progress">{controls.progressMessage}</p>
      ) : null}

      {controls.recoveryNote ? (
        <p className="node-operations-note is-recovery">{controls.recoveryNote}</p>
      ) : null}

      {controls.pendingConfirmation === "restart" ? (
        <div className="node-restart-confirmation" role="alert">
          <div className="node-restart-confirmation-icon">
            <AlertTriangle size={20} aria-hidden="true" />
          </div>
          <div className="node-restart-confirmation-copy">
            <strong>{controls.confirmationTitle}</strong>
            <p>{controls.confirmationBody}</p>
          </div>
          <div className="node-restart-confirmation-actions">
            <button
              className="is-confirm"
              onClick={() => actions.confirmRestartCore()}
              disabled={state.activeLifecycleAction != null}
            >
              <RotateCw size={15} aria-hidden="true" />
              Confirm restart
            </button>
            <button
              className="is-cancel"
              onClick={actions.cancelLifecycleConfirmation}
              disabled={state.activeLifecycleAction != null}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
