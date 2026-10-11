import type { QueryClient } from "@tanstack/react-query";

/** The current user's AI usage, mappings and declared subscriptions. */
export const AI_USAGE_QUERY_KEY = ["ai-usage"] as const;

/** Admin AI reports, mappings and everyone's declared subscriptions. */
export const AI_ADMIN_QUERY_KEY = ["ai-admin"] as const;

/**
 * Refreshes every AI read affected by a mapping or subscription mutation.
 * Personal actions can change an admin's own bill, and admin actions can
 * change the current user's personal view and settings.
 */
export async function invalidateAiQueries(queryClient: QueryClient) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: AI_USAGE_QUERY_KEY }),
    queryClient.invalidateQueries({ queryKey: AI_ADMIN_QUERY_KEY }),
  ]);
}
