import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { invalidateAiQueries } from "../ai-cache";
import { parseAiSubscription } from "../contracts/ai-usage";
import type { AiSubscription, AiSubscriptionTerms } from "../queries/ai-usage";
import type { DefaultMutationOptions } from "./mutations";

/** Input for declaring one of the current user's AI subscriptions. */
export type CreateAiSubscriptionVars = { terms: AiSubscriptionTerms };
/** Input for replacing the terms of one of the current user's subscriptions. */
export type UpdateAiSubscriptionVars = {
  subscriptionId: number;
  terms: AiSubscriptionTerms;
};
/** Input for deleting one of the current user's subscriptions. */
export type DeleteAiSubscriptionVars = { subscriptionId: number };

/** Mutation hooks for the current user's AI subscriptions. */
export const aiUsageMutations = {
  useCreateAiSubscription,
  useUpdateAiSubscription,
  useDeleteAiSubscription,
};

/** Declares a subscription and refreshes subscriptions and mismatches. */
export function useCreateAiSubscription(
  options?: DefaultMutationOptions<CreateAiSubscriptionVars, AiSubscription>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-usage", "subscriptions", "create"],
    mutationFn: async ({ terms }: CreateAiSubscriptionVars) =>
      parseAiSubscription(
        await api
          .post("ai-usage/subscriptions", { json: terms })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/** Replaces a subscription's terms and refreshes subscriptions and mismatches. */
export function useUpdateAiSubscription(
  options?: DefaultMutationOptions<UpdateAiSubscriptionVars, AiSubscription>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-usage", "subscriptions", "update"],
    mutationFn: async ({ subscriptionId, terms }: UpdateAiSubscriptionVars) =>
      parseAiSubscription(
        await api
          .put(`ai-usage/subscriptions/${subscriptionId}`, { json: terms })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/** Deletes a subscription and refreshes subscriptions and mismatches. */
export function useDeleteAiSubscription(
  options?: DefaultMutationOptions<DeleteAiSubscriptionVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-usage", "subscriptions", "delete"],
    mutationFn: async ({ subscriptionId }: DeleteAiSubscriptionVars) =>
      api.delete(`ai-usage/subscriptions/${subscriptionId}`),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}
