export type CoreManifest = {
  core_tag: string;
  consensus_tag: string;
  source_commit: string;
  binary_sha256: string;
  consensus_version: number;
  p2p_protocol_version: number;
  platform: string;
};

export type CoreVerification = {
  binary_path: string;
  expected_sha256: string;
  actual_sha256: string;
  matches: boolean;
};

export type DiagnosticsState = {
  manifest: CoreManifest | null;
  verification: CoreVerification | null;
  stdoutTail: string | null;
  stderrTail: string | null;
  error: string | null;
};
