export const AgentRunStatuses = [
  "created",
  "provisioningSandbox",
  "checkingRepositoryAccess",
  "cloningRepository",
  "loadingWorkflow",
  "planning",
  "awaitingPlanFeedback",
  "revisingPlan",
  "planApproved",
  "implementing",
  "verifying",
  "creatingDraftPr",
  "awaitingBackendPublish",
  "backendPublishing",
  "draftPrCreated",
  "succeeded",
  "failed",
  "canceled",
] as const;

export type AgentRunStatus = (typeof AgentRunStatuses)[number];

export const TerminalAgentRunStatuses = ["succeeded", "failed", "canceled"] as const satisfies ReadonlyArray<AgentRunStatus>;

const allowedTransitions = {
  created: ["provisioningSandbox", "failed", "canceled"],
  provisioningSandbox: ["checkingRepositoryAccess", "failed", "canceled"],
  checkingRepositoryAccess: ["cloningRepository", "failed", "canceled"],
  cloningRepository: ["loadingWorkflow", "failed", "canceled"],
  loadingWorkflow: ["planning", "failed", "canceled"],
  planning: ["awaitingPlanFeedback", "failed", "canceled"],
  awaitingPlanFeedback: ["revisingPlan", "planApproved", "failed", "canceled"],
  revisingPlan: ["awaitingPlanFeedback", "failed", "canceled"],
  planApproved: ["implementing", "failed", "canceled"],
  implementing: ["verifying", "failed", "canceled"],
  verifying: ["creatingDraftPr", "failed", "canceled"],
  creatingDraftPr: ["awaitingBackendPublish", "draftPrCreated", "failed", "canceled"],
  awaitingBackendPublish: ["backendPublishing", "failed", "canceled"],
  backendPublishing: ["draftPrCreated", "failed", "canceled"],
  draftPrCreated: ["succeeded", "failed", "canceled"],
  succeeded: [],
  failed: [],
  canceled: [],
} as const satisfies Record<AgentRunStatus, ReadonlyArray<AgentRunStatus>>;

export const isTerminalStatus = (status: AgentRunStatus): boolean =>
  (TerminalAgentRunStatuses as ReadonlyArray<AgentRunStatus>).includes(status);

export const canTransitionStatus = (from: AgentRunStatus, to: AgentRunStatus): boolean =>
  (allowedTransitions[from] as ReadonlyArray<AgentRunStatus>).includes(to);
