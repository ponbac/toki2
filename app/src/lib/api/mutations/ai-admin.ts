import { useMutation, useQueryClient } from "@tanstack/react-query";
import { HTTPError } from "ky";
import { api } from "../api";
import { AI_ADMIN_QUERY_KEY, invalidateAiQueries } from "../ai-cache";
import {
  parseAiExchangeRate,
  parseAiExchangeRateReset,
} from "../contracts/ai-admin";
import { parseMeteredImportStarted } from "../contracts/ai-metered-usage";
import {
  parseAiCloseConflict,
  parseAiMonthCloseStatus,
} from "../contracts/ai-month-close";
import {
  parseAcceptedSuggestions,
  parseAccountDisposition,
  parseDirectoryRefresh,
} from "../contracts/ai-provider-accounts";
import { parseAiRepositoryMapping } from "../contracts/ai-repository-mappings";
import { parseAiProjectMapping } from "../contracts/ai-usage-report";
import { parseAiSubscription } from "../contracts/ai-usage";
import {
  type AiAcceptedSuggestions,
  type AiAccountDisposition,
  type AiAccountDispositionTerms,
  type AiCloseCheckId,
  type AiCloseReadiness,
  type AiDirectoryRefresh,
  type AiExchangeRate,
  type AiMeteredImportMode,
  type AiMeteredImportStarted,
  type AiMonthCloseStatus,
  type AiProjectMapping,
  type AiRepositoryMapping,
} from "../queries/ai-admin";
import type { AiSubscription, AiSubscriptionTerms } from "../queries/ai-usage";
import type { DefaultMutationOptions } from "./mutations";

/** Maps a project key to a time-tracking project, or changes its mapping. */
export type SetProjectMappingVars = { projectKey: string; projectId: string };
export type DeleteProjectMappingVars = { projectKey: string };
/** Maps a repository the metered payer billed to a time-tracking project, or
 * changes its mapping. The name is matched case-insensitively. */
export type SetRepositoryMappingVars = {
  repository: string;
  projectId: string;
};
export type DeleteRepositoryMappingVars = { repository: string };
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
/** Gives a provider account a disposition. */
export type CreateAccountDispositionVars = {
  accountId: number;
  terms: AiAccountDispositionTerms;
};
/** Replaces a disposition's target and days; ending one sets its last day. */
export type UpdateAccountDispositionVars = {
  dispositionId: number;
  terms: AiAccountDispositionTerms;
};
export type DeleteAccountDispositionVars = { dispositionId: number };
/** Accepts verified suggestions: each account's top candidate, as the page
 * showed it. The server recomputes them and skips any that changed. */
export type AcceptAccountSuggestionsVars = {
  picks: readonly { accountId: number; userId: number }[];
};

/** Imports a UTC month of metered usage (plus months with gaps): its
 * changed days, or every day. */
export type StartMeteredImportVars = {
  month: string;
  mode: AiMeteredImportMode;
};

/** Closes a month, `YYYY-MM`, waiving the failed checks named with a reason
 * each. */
export type CloseMonthVars = {
  month: string;
  waivers: readonly { check: AiCloseCheckId; reason: string }[];
};
/** What closing a month came to: closed, or refused as not ready with the
 * readiness the server evaluated just then. */
export type CloseMonthResult =
  | Readonly<{ kind: "closed"; status: AiMonthCloseStatus }>
  | Readonly<{ kind: "notReady"; error: string; readiness: AiCloseReadiness }>;
export type ReopenMonthVars = { month: string };

/** The most picks the server takes in one request. */
const MAX_PICKS_PER_REQUEST = 100;

/** Admin mutations: mappings and subscriptions, which both change billing. */
export const aiAdminMutations = {
  useSetProjectMapping,
  useDeleteProjectMapping,
  useSetRepositoryMapping,
  useDeleteRepositoryMapping,
  useAdminCreateSubscription,
  useAdminUpdateSubscription,
  useAdminDeleteSubscription,
  useOverrideExchangeRate,
  useResetExchangeRate,
  useDownloadBillingCsv,
  useRefreshProviderAccounts,
  useCreateAccountDisposition,
  useUpdateAccountDisposition,
  useDeleteAccountDisposition,
  useAcceptAccountSuggestions,
  useStartMeteredImport,
  useCloseMonth,
  useReopenMonth,
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
  });
}

/**
 * Maps a repository to an active time-tracking project, or changes its
 * mapping. A new mapping needs a repository the payer billed (else 404); the
 * mapping applies whenever an open month's bill is read (a closed month gets
 * an adjustment on the next open month's), so every AI read refreshes.
 */
export function useSetRepositoryMapping(
  options?: DefaultMutationOptions<
    SetRepositoryMappingVars,
    AiRepositoryMapping
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "repository-mappings", "set"],
    mutationFn: async (body: SetRepositoryMappingVars) =>
      parseAiRepositoryMapping(
        await api
          .put("ai-usage/admin/repository-mappings", { json: body })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
  });
}

/** Removes a repository's mapping: its credits bill as Unassigned in open
 * months; a closed month gets an adjustment on the next open month's bill. */
export function useDeleteRepositoryMapping(
  options?: DefaultMutationOptions<DeleteRepositoryMappingVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "repository-mappings", "delete"],
    mutationFn: async (body: DeleteRepositoryMappingVars) =>
      api.delete("ai-usage/admin/repository-mappings", { json: body }),
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
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

/**
 * Reads seats and profiles from the payer's account directory now. A failure
 * is recorded on the server and shown on the page, so the page reloads either
 * way.
 */
export function useRefreshProviderAccounts(
  options?: DefaultMutationOptions<void, AiDirectoryRefresh>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "provider-accounts", "refresh"],
    mutationFn: async () =>
      parseDirectoryRefresh(
        await api
          .post("ai-usage/admin/provider-accounts/refresh", {
            // GitHub can take a while: one seat page and a profile per account.
            timeout: 120_000,
          })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
    onError: async (error, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onError?.(error, vars, ctx, mutationContext);
    },
  });
}

export function useCreateAccountDisposition(
  options?: DefaultMutationOptions<
    CreateAccountDispositionVars,
    AiAccountDisposition
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "provider-accounts", "dispositions", "create"],
    mutationFn: async ({ accountId, terms }: CreateAccountDispositionVars) =>
      parseAccountDisposition(
        await api
          .post("ai-usage/admin/provider-accounts/dispositions", {
            json: { accountId, ...terms },
          })
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
  });
}

export function useUpdateAccountDisposition(
  options?: DefaultMutationOptions<
    UpdateAccountDispositionVars,
    AiAccountDisposition
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "provider-accounts", "dispositions", "update"],
    mutationFn: async ({
      dispositionId,
      terms,
    }: UpdateAccountDispositionVars) =>
      parseAccountDisposition(
        await api
          .put(
            `ai-usage/admin/provider-accounts/dispositions/${dispositionId}`,
            { json: terms },
          )
          .json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
  });
}

export function useDeleteAccountDisposition(
  options?: DefaultMutationOptions<DeleteAccountDispositionVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "provider-accounts", "dispositions", "delete"],
    mutationFn: async ({ dispositionId }: DeleteAccountDispositionVars) =>
      api.delete(
        `ai-usage/admin/provider-accounts/dispositions/${dispositionId}`,
      ),
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
  });
}

/** Accepts verified suggestions, at most a hundred picks per request. */
export function useAcceptAccountSuggestions(
  options?: DefaultMutationOptions<
    AcceptAccountSuggestionsVars,
    AiAcceptedSuggestions
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "provider-accounts", "accept-suggestions"],
    mutationFn: async ({ picks }: AcceptAccountSuggestionsVars) => {
      const created: AiAcceptedSuggestions["created"][number][] = [];
      const skipped: AiAcceptedSuggestions["skipped"][number][] = [];
      for (let at = 0; at < picks.length; at += MAX_PICKS_PER_REQUEST) {
        const accepted = parseAcceptedSuggestions(
          await api
            .post("ai-usage/admin/provider-accounts/accept-suggestions", {
              json: { picks: picks.slice(at, at + MAX_PICKS_PER_REQUEST) },
            })
            .json<unknown>(),
        );
        created.push(...accepted.created);
        skipped.push(...accepted.skipped);
      }
      return { created, skipped };
    },
    ...options,
    onSettled: async (data, error, vars, ctx, mutationContext) => {
      // A later chunk can fail after earlier ones created dispositions.
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSettled?.(data, error, vars, ctx, mutationContext);
    },
  });
}

/**
 * Starts importing a month of metered usage from the payer. The server answers
 * at once and runs the import on its own; the month reloads until it is done.
 * Resolves to null when an import is already running (409), which is no error:
 * the page shows that run.
 */
export function useStartMeteredImport(
  options?: DefaultMutationOptions<
    StartMeteredImportVars,
    AiMeteredImportStarted | null
  >,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "metered-usage", "import"],
    mutationFn: async ({ month, mode }: StartMeteredImportVars) => {
      try {
        return parseMeteredImportStarted(
          await api
            .post(`ai-usage/admin/metered-usage/${month}/import`, {
              json: { mode },
            })
            .json<unknown>(),
        );
      } catch (error) {
        if (error instanceof HTTPError && error.response.status === 409) {
          return null;
        }
        throw error;
      }
    },
    ...options,
    onSuccess: async (data, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSuccess?.(data, vars, ctx, mutationContext);
    },
    onError: async (error, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onError?.(error, vars, ctx, mutationContext);
    },
  });
}

/**
 * Closes a month's whole AI bill. The server evaluates readiness again: a
 * month that is not ready (a check failed since the page loaded, or a waiver
 * no longer covers it) resolves to `notReady` with that readiness, which is no
 * error. Every other refusal (not over, already closed, another close
 * running) rejects. Either way every admin AI query reloads: a closed month
 * is served from its snapshot, and the next open month adjusts it.
 */
export function useCloseMonth(
  options?: DefaultMutationOptions<CloseMonthVars, CloseMonthResult>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "billing", "close"],
    mutationFn: async ({
      month,
      waivers,
    }: CloseMonthVars): Promise<CloseMonthResult> => {
      try {
        return {
          kind: "closed",
          status: parseAiMonthCloseStatus(
            await api
              .post(`ai-usage/admin/billing/${month}/close`, {
                json: { waivers },
                // Closing recomputes the month and every closed month posting
                // on it, and stores each line.
                timeout: 120_000,
              })
              .json<unknown>(),
          ),
        };
      } catch (error) {
        if (error instanceof HTTPError && error.response.status === 409) {
          const conflict = parseAiCloseConflict(error.data);
          if (conflict?.kind === "notReady") {
            return conflict;
          }
        }
        throw error;
      }
    },
    ...options,
    onSettled: async (data, error, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSettled?.(data, error, vars, ctx, mutationContext);
    },
  });
}

/**
 * Re-opens a closed month, so its bill is computed live again. The server
 * refuses (`409`, `heldBy`) while a close holds adjustments to the month.
 */
export function useReopenMonth(
  options?: DefaultMutationOptions<ReopenMonthVars, AiMonthCloseStatus>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-admin", "billing", "reopen"],
    mutationFn: async ({ month }: ReopenMonthVars) =>
      parseAiMonthCloseStatus(
        await api
          .delete(`ai-usage/admin/billing/${month}/close`, { timeout: 60_000 })
          .json<unknown>(),
      ),
    ...options,
    onSettled: async (data, error, vars, ctx, mutationContext) => {
      await queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY });
      await options?.onSettled?.(data, error, vars, ctx, mutationContext);
    },
  });
}
