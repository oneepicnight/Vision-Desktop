import type { DashboardSnapshot, NodeConfig, ProcessState } from "../types/core";

export type DesktopEvent =
  | { type: "MockModeChanged"; mockMode: boolean }
  | { type: "WizardOpenChanged"; open: boolean }
  | { type: "NodeConfigChanged"; config: NodeConfig }
  | { type: "DashboardRefreshStarted" }
  | { type: "DashboardSnapshotUpdated"; snapshot: DashboardSnapshot; message: string }
  | { type: "CoreProcessUpdated"; process: ProcessState }
  | { type: "DesktopUpdateSettled" }
  | { type: "DesktopActionStarted"; name: string }
  | { type: "DesktopActionCompleted"; name: string }
  | { type: "DesktopActionFailed"; message: string };
