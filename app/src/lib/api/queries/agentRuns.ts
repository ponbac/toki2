import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export type AgentRunStatus =
  | "created"
  | "provisioningSandbox"
  | "checkingRepositoryAccess"
  | "cloningRepository"
  | "loadingWorkflow"
  | "planning"
  | "awaitingPlanFeedback"
  | "revisingPlan"
  | "planApproved"
  | "implementing"
  | "verifying"
  | "creatingDraftPr"
  | "awaitingBackendPublish"
  | "backendPublishing"
  | "draftPrCreated"
  | "succeeded"
  | "failed"
  | "canceled";

export type AgentRunEvent = {
  id: string;
  runId: string;
  status: AgentRunStatus;
  message: string;
  createdAt: string;
};

export type AgentRunFeedback = {
  id: string;
  message: string;
  createdAt: string;
  actor: {
    tokiUserId: number;
    displayName: string;
  };
};

export type AgentRunRecord = {
  id: string;
  status: AgentRunStatus;
  mode: "planFirst";
  source: {
    type: "adoWorkItem" | "githubIssue";
    id: string;
    title: string;
    url: string;
    markdown: string;
  };
  targetRepo: {
    provider: "azureDevOps" | "github";
    cloneUrl: string;
    defaultBranch: string;
    organization?: string;
    project?: string;
    repoName?: string;
  };
  metadata?: {
    model?: string;
    reasoningLevel?: string;
    tokenUsage?: {
      inputTokens?: number;
      outputTokens?: number;
      totalTokens?: number;
    };
  };
  workpad: {
    currentPlanMarkdown: string;
    planVersion: number;
    feedbackHistory: AgentRunFeedback[];
    acceptanceCriteria: string[];
    validationChecklist: string[];
    notes: string[];
    risksAndConfusions: string[];
    finalSummary?: string;
    draftPrUrl?: string;
  };
  events: AgentRunEvent[];
  createdAt: string;
  updatedAt: string;
};

export type AgentRunIssueSummary = {
  id: string;
  workItemId: string;
  status: AgentRunStatus;
  draftPrUrl?: string;
  createdBy: {
    displayName: string;
  };
  createdAt: string;
  updatedAt: string;
  lastSyncedAt: string;
  syncState: "fresh" | "stale";
};

export type LatestAgentRunsByWorkItemsResponse = {
  runs: AgentRunIssueSummary[];
};

export const agentRunQueries = {
  baseKey: ["agentRuns"] as const,
  run: (id: string) =>
    queryOptions({
      queryKey: [...agentRunQueries.baseKey, id],
      queryFn: async () => api.get(`agent-runs/${id}`).json<AgentRunRecord>(),
      refetchInterval: 3_000,
    }),
  events: (id: string) =>
    queryOptions({
      queryKey: [...agentRunQueries.baseKey, id, "events"],
      queryFn: async () =>
        api.get(`agent-runs/${id}/events`).json<AgentRunEvent[]>(),
      refetchInterval: 3_000,
    }),
  latestByWorkItems: (params: {
    sourceProvider: "azureDevOpsWorkItem";
    organization: string;
    project: string;
    workItemIds: string[];
  }) =>
    queryOptions({
      queryKey: [...agentRunQueries.baseKey, "latestByWorkItems", params],
      queryFn: async () =>
        api
          .post("agent-runs/latest-by-work-items", { json: params })
          .json<LatestAgentRunsByWorkItemsResponse>(),
      enabled: params.workItemIds.length > 0,
      refetchInterval: 15_000,
    }),
};
