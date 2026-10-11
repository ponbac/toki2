import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { AI_ADMIN_QUERY_KEY, invalidateAiQueries } from "../ai-cache";
import {
  parseAiExchangeRate,
  parseAiExchangeRateReset,
} from "../contracts/ai-admin";
import { parseAiProjectMapping } from "../contracts/ai-usage-report";
import { parseAiSubscription } from "../contracts/ai-usage";
import {
  type AiExchangeRate,
  type AiProjectMapping,
} from "../queries/ai-admin";
import type { AiSubscription, AiSubscriptionTerms } from "../queries/ai-usage";
import type { DefaultMutationOptions } from "./mutations";

/** Maps a project key to a time-tracking project, or changes its mapping. */
export type SetProjectMappingVars = { projectKey: string; projectId: string };
export type DeleteProjectMappingVars = { projectKey: string };
/** Declares a subscription for any user. */
export type AdminCreateSubscriptionVars = {
  userId: number;
  terms: AiSubscriptionTerms;
};
export type AdminUpdateSubscriptionVars = {
  subscriptionId: number;
  terms: AiSubscriptionTerms;
};
export type AdminDeleteSubscriptionVars = { subscriptionId: number };
/** Overrides a month's exchange rate: `rate` units of the billing currency
 * per unit of `currency`, such as `"10.25"`. */
export type OverrideExchangeRateVars = {
  month: string;
  currency: string;
  rate: string;
};
export type ResetExchangeRateVars = { month: string; currency: string };

/** Admin mutations: mappings and subscriptions, which both change billing. */
export const aiAdminMutations = {
  useSetProjectMapping,
  useDeleteProjectMapping,
  useAdminCreateSubscription,
  useAdminUpdateSubscription,
  useAdminDeleteSubscription,
  useOverrideExchangeRate,
  useResetExchangeRate,
  useDownloadBillingCsv,
};

export function useOverrideExchangeRate(
  options?: DefaultMutationOptions<OverrideExchangeRateVars, AiExchangeRate>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "exchange-rates", "override"],
    mutationFn: async ({ month, currency, rate }: OverrideExchangeRateVars) =>
      parseAiExchangeRate(
        await api
          .put(`ai-usage/admin/billing/${month}/exchange-rates/${currency}`, {
            json: { rate },
          })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/** Removes an override; the server fetches the Riksbank rate again. */
export function useResetExchangeRate(
  options?: DefaultMutationOptions<
    ResetExchangeRateVars,
    AiExchangeRate | null
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "exchange-rates", "reset"],
    mutationFn: async ({ month, currency }: ResetExchangeRateVars) =>
      parseAiExchangeRateReset(
        await api
          .delete(`ai-usage/admin/billing/${month}/exchange-rates/${currency}`)
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useSetProjectMapping(
  options?: DefaultMutationOptions<SetProjectMappingVars, AiProjectMapping>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "project-mappings", "set"],
    mutationFn: async (body: SetProjectMappingVars) =>
      parseAiProjectMapping(
        await api
          .put("ai-usage/project-mappings", { json: body })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useDeleteProjectMapping(
  options?: DefaultMutationOptions<DeleteProjectMappingVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "project-mappings", "delete"],
    mutationFn: async (body: DeleteProjectMappingVars) =>
      api.delete("ai-usage/project-mappings", { json: body }),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useAdminCreateSubscription(
  options?: DefaultMutationOptions<AdminCreateSubscriptionVars, AiSubscription>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "subscriptions", "create"],
    mutationFn: async ({ userId, terms }: AdminCreateSubscriptionVars) =>
      parseAiSubscription(
        await api
          .post("ai-usage/admin/subscriptions", { json: { userId, ...terms } })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useAdminUpdateSubscription(
  options?: DefaultMutationOptions<AdminUpdateSubscriptionVars, AiSubscription>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "subscriptions", "update"],
    mutationFn: async ({
      subscriptionId,
      terms,
    }: AdminUpdateSubscriptionVars) =>
      parseAiSubscription(
        await api
          .put(`ai-usage/admin/subscriptions/${subscriptionId}`, {
            json: terms,
          })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useAdminDeleteSubscription(
  options?: DefaultMutationOptions<AdminDeleteSubscriptionVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "subscriptions", "delete"],
    mutationFn: async ({ subscriptionId }: AdminDeleteSubscriptionVars) =>
      api.delete(`ai-usage/admin/subscriptions/${subscriptionId}`),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/**
 * Downloads a month's billing CSV. It is fetched with the session cookie, like
 * every admin request, and saved through a temporary object URL.
 */
export function useDownloadBillingCsv(
  options?: DefaultMutationOptions<{ month: string }, void>,
) {
  return useMutation({
    mutationKey: ["ai-admin", "billing", "export"],
    mutationFn: async ({ month }: { month: string }) => {
      const blob = await api
        .get(`ai-usage/admin/billing/${month}/export.csv`)
        .blob();
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `ai-usage-billing-${month}.csv`;
      document.body.append(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
    },
    ...options,
  });
}
