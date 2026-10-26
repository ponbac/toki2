import { queryOptions } from "@tanstack/react-query";
import { z } from "zod";
import { api } from "../api";
import { AI_ADMIN_QUERY_KEY } from "../ai-cache";
import {
  whileRatesArePending,
  whileReadinessAwaitsRates,
  withoutFrozenFetches,
} from "../ai-billing-refresh";
import {
  parseAiBillingCompleteness,
  parseAiBillingDevelopers,
  parseAiBillingOverview,
  parseAiDeveloperMonth,
  parseAiUnmappedProjectKeys,
} from "../contracts/ai-admin";
import { parseMeteredMonth } from "../contracts/ai-metered-usage";
import { parseAiMonthCloseStatus } from "../contracts/ai-month-close";
import { parseProviderAccounts } from "../contracts/ai-provider-accounts";
import { parseAiRepositoryMappings } from "../contracts/ai-repository-mappings";
import { parseAiProjectMapping } from "../contracts/ai-usage-report";
import {
  parseAiSubscriptionList,
  parseAiSubscriptionMismatchList,
} from "../contracts/ai-usage";

export type {
  AiAdjustedKind,
  AiAdjustedMonth,
  AiAdjustmentLine,
  AiBillingCompleteness,
  AiBillingDeveloper,
  AiBillingLine,
  AiBillingOverview,
  AiBillingProject,
  AiBillingSubject,
  AiBillingTotals,
  AiBillingUsage,
  AiDayUsage,
  AiDeveloperCompleteness,
  AiDeveloperMonth,
  AiExchangeRate,
  AiMachineCompleteness,
  AiMeteredBill,
  AiMeteredLine,
  AiMeteredLineKind,
  AiMeteredPeriod,
  AiMeteredProvisionalReason,
  AiMonthClose,
  AiSubscriptionMonth,
  AiUnmappedProjectKey,
  AiUploadGap,
} from "../contracts/ai-admin";
export type {
  AiMeteredAccount,
  AiMeteredAccountError,
  AiMeteredChargeKind,
  AiMeteredCheck,
  AiMeteredFlag,
  AiMeteredGroup,
  AiMeteredGroupKind,
  AiMeteredImport,
  AiMeteredImportMode,
  AiMeteredImportStarted,
  AiMeteredMonth,
  AiMeteredOwner,
  AiMeteredPool,
  AiMeteredRepository,
  AiMeteredRun,
  AiMeteredSourceKind,
  AiMeteredStaleReason,
  AiMeteredTotals,
  AiMeteredUnlinkedTracked,
  AiCodeReviewDay,
  AiResidualDay,
  AiResidualStatus,
} from "../contracts/ai-metered-usage";
export type {
  AiAcceptedSuggestions,
  AiAccountDirectory,
  AiAccountDisposition,
  AiAccountDispositionTerms,
  AiAccountPeriod,
  AiAccountSuggestion,
  AiAccountTarget,
  AiDirectoryRefresh,
  AiMeteredActivity,
  AiProviderAccount,
  AiProviderAccounts,
  AiProviderSeat,
  AiSuggestionEvidence,
  AiSuggestionSkipReason,
  AiUnlinkedUser,
} from "../contracts/ai-provider-accounts";
export type {
  AiRepositoryMapping,
  AiRepositoryMappingRow,
  AiRepositoryMappingStatus,
  AiRepositoryMappings,
  AiRepositoryMonthCredits,
  AiRepositorySuggestion,
} from "../contracts/ai-repository-mappings";
export type {
  AiCloseCheck,
  AiCloseCheckId,
  AiCloseCheckStatus,
  AiCloseConflict,
  AiCloseHistoryEntry,
  AiCloseProblem,
  AiCloseProblemKind,
  AiCloseReadiness,
  AiMonthCloseStatus,
} from "../contracts/ai-month-close";
export type { AiProjectMapping } from "../contracts/ai-usage-report";

/** How often a month whose import is running is reloaded. */
const RUNNING_IMPORT_POLL_MS = 3_000;

/** Reloads a metered month until its running import has finished. */
function whileImportRuns(query: {
  state: { data?: { import: { running: unknown } } };
}): number | false {
  return query.state.data?.import.running ? RUNNING_IMPORT_POLL_MS : false;
}

export { AI_ADMIN_QUERY_KEY } from "../ai-cache";

const keys = {
  billing: (month: string) =>
    [...AI_ADMIN_QUERY_KEY, "billing", month] as const,
  monthClose: (month: string) =>
    [...AI_ADMIN_QUERY_KEY, "billing", month, "close"] as const,
  completeness: (month: string) =>
    [...AI_ADMIN_QUERY_KEY, "completeness", month] as const,
  developerMonth: (month: string, userId: number) =>
    [...AI_ADMIN_QUERY_KEY, "billing", month, "developer", userId] as const,
  developers: [...AI_ADMIN_QUERY_KEY, "developers"] as const,
  mappings: [...AI_ADMIN_QUERY_KEY, "project-mappings"] as const,
  unmappedKeys: [...AI_ADMIN_QUERY_KEY, "unmapped-keys"] as const,
  repositoryMappings: [...AI_ADMIN_QUERY_KEY, "repository-mappings"] as const,
  subscriptions: [...AI_ADMIN_QUERY_KEY, "subscriptions"] as const,
  mismatches: [...AI_ADMIN_QUERY_KEY, "subscription-mismatches"] as const,
  providerAccounts: [...AI_ADMIN_QUERY_KEY, "provider-accounts"] as const,
  meteredMonth: (month: string) =>
    [...AI_ADMIN_QUERY_KEY, "metered-usage", month] as const,
  meteredDeveloper: (month: string, userId: number) =>
    [
      ...AI_ADMIN_QUERY_KEY,
      "metered-usage",
      month,
      "developer",
      userId,
    ] as const,
};

/**
 * Admin-only AI usage queries. The server answers only an admin signed in with
 * a browser session; everyone else gets 403.
 */
export const aiAdminQueries = {
  /** Everyone's bill for a month, `YYYY-MM`. */
  billing: (month: string) =>
    queryOptions({
      queryKey: keys.billing(month),
      queryFn: async () =>
        withoutFrozenFetches(
          parseAiBillingOverview(
            await api.get(`ai-usage/admin/billing/${month}`).json<unknown>(),
          ),
        ),
      refetchInterval: whileRatesArePending,
    }),
  /**
   * Where a month stands with close: in progress (and when it is over),
   * open with the server's readiness checks, or closed, with its frozen
   * readiness and the months that hold adjustments to it.
   */
  monthClose: (month: string) =>
    queryOptions({
      queryKey: keys.monthClose(month),
      queryFn: async () =>
        parseAiMonthCloseStatus(
          await api
            .get(`ai-usage/admin/billing/${month}/close`)
            .json<unknown>(),
        ),
      refetchInterval: whileReadinessAwaitsRates,
    }),
  /** Whether each developer's machines uploaded all of a month's usage. */
  completeness: (month: string) =>
    queryOptions({
      queryKey: keys.completeness(month),
      queryFn: async () =>
        parseAiBillingCompleteness(
          await api
            .get(`ai-usage/admin/billing/${month}/completeness`)
            .json<unknown>(),
        ),
    }),
  /** One developer's month, down to the day. */
  developerMonth: (month: string, userId: number) =>
    queryOptions({
      queryKey: keys.developerMonth(month, userId),
      queryFn: async () =>
        withoutFrozenFetches(
          parseAiDeveloperMonth(
            await api
              .get(`ai-usage/admin/billing/${month}/developers/${userId}`)
              .json<unknown>(),
          ),
        ),
      refetchInterval: whileRatesArePending,
    }),
  /** Every user, by name. */
  developers: () =>
    queryOptions({
      queryKey: keys.developers,
      queryFn: async () =>
        parseAiBillingDevelopers(
          await api.get("ai-usage/admin/developers").json<unknown>(),
        ),
    }),
  /** Every project-key mapping, including stale, unconfigured and unused ones. */
  projectMappings: () =>
    queryOptions({
      queryKey: keys.mappings,
      queryFn: async () =>
        z
          .array(z.unknown())
          .parse(await api.get("ai-usage/project-mappings").json<unknown>())
          .map(parseAiProjectMapping),
    }),
  /** Keys in anyone's usage without a mapping, most recently used first. */
  unmappedKeys: () =>
    queryOptions({
      queryKey: keys.unmappedKeys,
      queryFn: async () =>
        parseAiUnmappedProjectKeys(
          await api
            .get("ai-usage/project-mappings/unmapped-keys")
            .json<unknown>(),
        ),
    }),
  /**
   * The repositories the metered payer (GitHub Copilot) billed code review
   * credits to, and every repository mapping, with credits in the previous
   * and current UTC month.
   */
  repositoryMappings: () =>
    queryOptions({
      queryKey: keys.repositoryMappings,
      queryFn: async () =>
        parseAiRepositoryMappings(
          await api.get("ai-usage/admin/repository-mappings").json<unknown>(),
        ),
    }),
  /** Everyone's declared AI subscriptions. */
  subscriptions: () =>
    queryOptions({
      queryKey: keys.subscriptions,
      queryFn: async () =>
        parseAiSubscriptionList(
          await api.get("ai-usage/admin/subscriptions").json<unknown>(),
        ),
    }),
  /**
   * Everyone's undeclared-subscription evidence on the local days `from`
   * through `to`, both `YYYY-MM-DD` and inclusive. The server groups evidence
   * into runs that may span days without any, so a run's bounding dates do
   * not show that a month had evidence: ask for the month itself.
   */
  subscriptionMismatchesIn: (from: string, to: string) =>
    queryOptions({
      queryKey: [...keys.mismatches, from, to] as const,
      queryFn: async () =>
        parseAiSubscriptionMismatchList(
          await api
            .get("ai-usage/admin/subscription-mismatches", {
              searchParams: { from, to },
            })
            .json<unknown>(),
        ),
    }),
  /**
   * The accounts holding seats at the metered payer (GitHub Copilot), with
   * their seats, dispositions and suggestions, and what needs a look.
   */
  providerAccounts: () =>
    queryOptions({
      queryKey: keys.providerAccounts,
      queryFn: async () =>
        parseProviderAccounts(
          await api.get("ai-usage/admin/provider-accounts").json<unknown>(),
        ),
    }),
  /**
   * What the metered payer (GitHub Copilot) billed in a UTC month, `YYYY-MM`,
   * reconciled with Toki's tracked usage, with the import's state. Reloads
   * while an import runs.
   */
  meteredMonth: (month: string) =>
    queryOptions({
      queryKey: keys.meteredMonth(month),
      queryFn: async () =>
        parseMeteredMonth(
          await api
            .get(`ai-usage/admin/metered-usage/${month}`)
            .json<unknown>(),
        ),
      refetchInterval: whileImportRuns,
    }),
  /** The same month, restricted to the accounts linked to one user. */
  meteredDeveloper: (month: string, userId: number) =>
    queryOptions({
      queryKey: keys.meteredDeveloper(month, userId),
      queryFn: async () =>
        parseMeteredMonth(
          await api
            .get(`ai-usage/admin/metered-usage/${month}/developers/${userId}`)
            .json<unknown>(),
        ),
      refetchInterval: whileImportRuns,
    }),
  /** Everyone's undeclared-subscription evidence over the last 90 days. */
  subscriptionMismatches: () =>
    queryOptions({
      queryKey: keys.mismatches,
      queryFn: async () =>
        parseAiSubscriptionMismatchList(
          await api
            .get("ai-usage/admin/subscription-mismatches")
            .json<unknown>(),
        ),
    }),
};
