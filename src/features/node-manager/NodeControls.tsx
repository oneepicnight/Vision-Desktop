import { Play, RefreshCw, RotateCw, Square } from "lucide-react";
import type { DesktopActions, DesktopState } from "../../state/desktopState";

type NodeControlsProps = {
  state: DesktopState;
  actions: DesktopActions;
};

export function NodeControls({ state, actions }: NodeControlsProps) {
  return (
    <div className="actions">
      <button onClick={actions.startCore} disabled={state.mockMode}>
        <Play size={18} />Start
      </button>
      <button onClick={actions.stopCore} disabled={state.mockMode}>
        <Square size={18} />Stop
      </button>
      <button onClick={actions.restartCore} disabled={state.mockMode}>
        <RotateCw size={18} />Restart
      </button>
      <button onClick={actions.refresh}>
        <RefreshCw size={18} />Refresh
      </button>
    </div>
  );
}
