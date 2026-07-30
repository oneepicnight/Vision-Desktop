import { Play, RefreshCw, RotateCw, Square } from "lucide-react";
import { restartCore, startCore, stopCore } from "../../services/coreApi";
import type { AppAction } from "../../types/ui";

type NodeControlsProps = {
  mockMode: boolean;
  refresh: () => Promise<void>;
  action: AppAction;
};

export function NodeControls({ mockMode, refresh, action }: NodeControlsProps) {
  return (
    <div className="actions">
      <button onClick={() => action("Start", startCore)} disabled={mockMode}>
        <Play size={18} />Start
      </button>
      <button onClick={() => action("Stop", stopCore)} disabled={mockMode}>
        <Square size={18} />Stop
      </button>
      <button onClick={() => action("Restart", restartCore)} disabled={mockMode}>
        <RotateCw size={18} />Restart
      </button>
      <button onClick={refresh}>
        <RefreshCw size={18} />Refresh
      </button>
    </div>
  );
}
