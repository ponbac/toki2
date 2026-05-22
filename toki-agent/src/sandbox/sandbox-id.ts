export const sandboxIdFromRunId = (runId: string): string =>
  `run-${runId.replaceAll(/[^a-zA-Z0-9-]/g, "-").toLowerCase().slice(0, 32)}`;

export const defaultSandboxSessionId = (sandboxId: string): string => `sandbox-${sandboxId}`;
