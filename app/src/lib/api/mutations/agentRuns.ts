import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { agentRunQueries, type AgentRunRecord } from "../queries/agentRuns";
import type { DefaultMutationOptions } from "./mutations";

export type CreateAgentRunPayload = {
  source: {
    id: string;
    title: string;
    url: string;
    organization: string;
    project: string;
  };
  targetRepo: {
    provider: "azureDevOps";
    organization: string;
    project: string;
    repoName: string;
    defaultBranch: string;
  };
  prompt?: string;
};

export type AgentRunFeedbackPayload = {
  id: string;
  message: string;
};

export const agentRunMutations = {
  useCreateAgentRun,
  useSendAgentRunFeedback,
  useApproveAgentRunPlan,
  useCancelAgentRun,
  useDeleteAgentRun,
};

function useCreateAgentRun(
  options?: DefaultMutationOptions<CreateAgentRunPayload, AgentRunRecord>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["agent-runs", "create"],
    mutationFn: async (body: CreateAgentRunPayload) =>
      api
        .post("agent-runs", {
          json: body,
        })
        .json<AgentRunRecord>(),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.setQueryData(agentRunQueries.run(data.id).queryKey, data);
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useSendAgentRunFeedback(
  options?: DefaultMutationOptions<AgentRunFeedbackPayload, AgentRunRecord>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["agent-runs", "feedback"],
    mutationFn: async ({ id, message }: AgentRunFeedbackPayload) =>
      api
        .post(`agent-runs/${id}/feedback`, {
          json: { message },
        })
        .json<AgentRunRecord>(),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.setQueryData(agentRunQueries.run(data.id).queryKey, data);
      queryClient.invalidateQueries(agentRunQueries.events(data.id));
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useApproveAgentRunPlan(
  options?: DefaultMutationOptions<string, AgentRunRecord>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["agent-runs", "approve-plan"],
    mutationFn: async (id: string) =>
      api.post(`agent-runs/${id}/approve-plan`).json<AgentRunRecord>(),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.setQueryData(agentRunQueries.run(data.id).queryKey, data);
      queryClient.invalidateQueries(agentRunQueries.events(data.id));
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useCancelAgentRun(
  options?: DefaultMutationOptions<string, AgentRunRecord>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["agent-runs", "cancel"],
    mutationFn: async (id: string) =>
      api.post(`agent-runs/${id}/cancel`).json<AgentRunRecord>(),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.setQueryData(agentRunQueries.run(data.id).queryKey, data);
      queryClient.invalidateQueries(agentRunQueries.events(data.id));
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useDeleteAgentRun(options?: DefaultMutationOptions<string, void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["agent-runs", "delete"],
    mutationFn: async (id: string) => {
      await api.delete(`agent-runs/${id}`);
    },
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.removeQueries({
        queryKey: agentRunQueries.run(vars).queryKey,
      });
      queryClient.invalidateQueries({
        queryKey: agentRunQueries.baseKey,
      });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}
