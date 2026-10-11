import { queryOptions } from "@tanstack/react-query";
import { z } from "zod";
import { api } from "../api";
import { AI_ADMIN_QUERY_KEY } from "../ai-cache";
import {
  parseAiBillingCompleteness,
  parseAiBillingDevelopers,
  parseAiBillingOverview,
  parseAiDeveloperMonth,
  parseAiUnmappedProjectKeys,
} from "../contracts/ai-admin";
import { parseAiProjectMapping } from "../contracts/ai-usage-report";
import {
  parseAiSubscriptionList,
  parseAiSubscriptionMismatchList,
} from "../contracts/ai-usage";

export type {
  AiBillingCompleteness,
  AiBillingDeveloper,
  AiBillingLine,
  AiBillingOverview,
  AiBillingProject,
  AiBillingTotals,
  AiBillingUsage,
  AiDayUsage,
  AiDeveloperCompleteness,
  AiDeveloperMonth,
  AiMachineCompleteness,
  AiSubscriptionMonth,
  AiUnmappedProjectKey,
  AiUploadGap,
} from "../contracts/ai-admin";
export type { AiProjectMapping } from "../contracts/ai-usage-report";

export { AI_ADMIN_QUERY_KEY } from "../ai-cache";

const keys = {
  billing: (month: string) =>
    [...AI_ADMIN_QUERY_KEY, "billing", month] as const,
  completeness: (month: string) =>
    [...AI_ADMIN_QUERY_KEY, "completeness", month] as const,
  developerMonth: (month: string, userId: number) =>
    [...AI_ADMIN_QUERY_KEY, "billing", month, "developer", userId] as const,
  developers: [...AI_ADMIN_QUERY_KEY, "developers"] as const,
  mappings: [...AI_ADMIN_QUERY_KEY, "project-mappings"] as const,
  unmappedKeys: [...AI_ADMIN_QUERY_KEY, "unmapped-keys"] as const,
  subscriptions: [...AI_ADMIN_QUERY_KEY, "subscriptions"] as const,
  mismatches: [...AI_ADMIN_QUERY_KEY, "subscription-mismatches"] as const,
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
        parseAiBillingOverview(
          await api.get(`ai-usage/admin/billing/${month}`).json<unknown>(),
        ),
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
        parseAiDeveloperMonth(
          await api
            .get(`ai-usage/admin/billing/${month}/developers/${userId}`)
            .json<unknown>(),
        ),
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
  /** Everyone's declared AI subscriptions. */
  subscriptions: () =>
    queryOptions({
      queryKey: keys.subscriptions,
      queryFn: async () =>
        parseAiSubscriptionList(
          await api.get("ai-usage/admin/subscriptions").json<unknown>(),
        ),
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
