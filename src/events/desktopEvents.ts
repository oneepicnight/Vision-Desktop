import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";
import type { ConfigurationState } from "../types/configuration";
import type { DiagnosticsState } from "../types/diagnostics";
import type {
  DesktopView,
  ExplorerLookupMode,
  ExplorerResult,
} from "../types/explorer";
import type { WalletAccountState } from "../types/wallet";
import type { LifecycleActionKind } from "../features/node-manager/lifecycleControls";

export type DesktopEvent =
  | { type: "ActiveViewChanged"; view: DesktopView }
  | { type: "MockModeChanged"; mockMode: boolean }
  | { type: "WizardOpenChanged"; open: boolean }
  | { type: "NodeConfigChanged"; config: NodeConfig }
  | { type: "ExplorerModeChanged"; mode: ExplorerLookupMode }
  | { type: "ExplorerQueryChanged"; query: string }
  | { type: "ExplorerLookupStarted"; message: string }
  | { type: "ExplorerResultUpdated"; result: ExplorerResult; message: string }
  | { type: "ExplorerLookupCleared" }
  | { type: "ExplorerLookupFailed"; message: string }
  | { type: "DashboardRefreshStarted" }
  | {
      type: "DashboardSnapshotUpdated";
      snapshot: DashboardSnapshot;
      message: string;
      receivedAt: number;
    }
  | { type: "DiagnosticsUpdated"; diagnostics: DiagnosticsState }
  | { type: "ConfigurationUpdated"; configuration: ConfigurationState }
  | { type: "WalletAccountUpdated"; wallet: WalletAccountState }
  | { type: "CoreProcessUpdated"; process: ProcessState }
  | { type: "LifecycleConfirmationRequested"; action: LifecycleActionKind }
  | { type: "LifecycleConfirmationDismissed" }
  | { type: "LifecycleActionStarted"; action: LifecycleActionKind; message: string }
  | { type: "LifecycleActionCompleted"; action: LifecycleActionKind; message: string }
  | { type: "LifecycleActionFailed"; action: LifecycleActionKind; message: string }
  | { type: "DesktopUpdateSettled" }
  | { type: "DesktopActionStarted"; name: string }
  | { type: "DesktopActionCompleted"; name: string }
  | { type: "DesktopActionFailed"; message: string };