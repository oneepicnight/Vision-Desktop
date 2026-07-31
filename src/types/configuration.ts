import type { NodeConfig } from "./core";

export type ConfigurationSourceKind = "persisted" | "desktop_default_created";

export type NodeConfigSnapshot = {
  config: NodeConfig;
  source_path: string;
  source_kind: ConfigurationSourceKind;
};

export type AppPaths = {
  desktop_config: string;
  node_config: string;
  core_data: string;
  core_logs: string;
  desktop_logs: string;
  reports: string;
  updates: string;
};

export type ConfigurationState = {
  snapshot: NodeConfigSnapshot | null;
  appPaths: AppPaths | null;
  error: string | null;
};
