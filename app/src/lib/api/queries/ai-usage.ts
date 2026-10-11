import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import {
  parseAiSubscriptionList,
  parseAiSubscriptionMismatchList,
} from "../contracts/ai-usage";

export {
  AI_PROVIDERS,
  MAX_PLAN_CODE_POINTS,
  codePointLength,
  type AiProvider,
  type AiSubscription,
  type AiSubscriptionList,
  type AiSubscriptionMismatch,
  type AiSubscriptionMismatchList,
  type AiSubscriptionTerms,
} from "../contracts/ai-usage";

const aiUsageQueryKeys = {
  subscriptions: ["ai-usage", "subscriptions"] as const,
  subscriptionMismatches: ["ai-usage", "subscription-mismatches"] as const,
};

/** Query definitions for the current user's AI subscriptions. */
export const aiUsageQueries = {
  aiSubscriptions: () =>
    queryOptions({
      queryKey: aiUsageQueryKeys.subscriptions,
      queryFn: async () =>
        parseAiSubscriptionList(
          await api.get("ai-usage/subscriptions").json<unknown>(),
        ),
    }),
  /** Mismatches over the server's default range: the last 90 local days. */
  aiSubscriptionMismatches: () =>
    queryOptions({
      queryKey: aiUsageQueryKeys.subscriptionMismatches,
      queryFn: async () =>
        parseAiSubscriptionMismatchList(
          await api.get("ai-usage/subscription-mismatches").json<unknown>(),
        ),
    }),
};
