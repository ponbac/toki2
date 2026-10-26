import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { invalidateAiQueries } from "../ai-cache";
import { parseAiProjectMapping } from "../contracts/ai-usage-report";
import type { AiProjectMapping } from "../queries/ai-usage-report";
import type { DefaultMutationOptions } from "./mutations";

/** Input for mapping an unmapped project key in the user's own usage. */
export type MapAiProjectKeyVars = Readonly<{
  projectKey: string;
  projectId: string;
}>;

/** Input for marking a project key as personal, or removing that mark. */
export type PersonalAiProjectKeyVars = Readonly<{ projectKey: string }>;

/** Mutation hooks for project mappings made from the personal usage view. */
export const aiProjectMappingMutations = {
  useMapAiProjectKey,
  useMarkAiProjectKeyPersonal,
  useUnmarkAiProjectKeyPersonal,
};

/**
 * Maps an unmapped key in the user's usage to a time-tracking project. The
 * server refuses keys that are already mapped (changing one takes an admin)
 * and keys outside the user's usage. Mappings apply to every open month (a
 * closed month gets an adjustment on the next open month's bill), so every
 * personal usage query refreshes once the request settles: a refusal, such as
 * someone else mapping the key first, refreshes them too.
 */
export function useMapAiProjectKey(
  options?: DefaultMutationOptions<MapAiProjectKeyVars, AiProjectMapping>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-usage", "project-mappings", "map"],
    mutationFn: async (vars: MapAiProjectKeyVars) =>
      parseAiProjectMapping(
        await api
          .put("ai-usage/project-mappings", { json: vars })
          .json<unknown>(),
      ),
    ...options,
    onSettled: async (data, error, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSettled?.(data, error, vars, ctx, mutationContext);
    },
  });
}

/**
 * Marks an unmapped key in the user's usage as personal: not client work. The
 * user's usage of it is then left out of every open month's bill (a closed
 * month gets an adjustment on the next open month's bill), so every personal
 * usage query refreshes once the request settles.
 */
export function useMarkAiProjectKeyPersonal(
  options?: DefaultMutationOptions<PersonalAiProjectKeyVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-usage", "project-mappings", "personal", "mark"],
    mutationFn: async (vars: PersonalAiProjectKeyVars) =>
      api.put("ai-usage/project-mappings/personal", { json: vars }),
    ...options,
    onSettled: async (data, error, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSettled?.(data, error, vars, ctx, mutationContext);
    },
  });
}

/** Removes the user's personal mark of a key, leaving its usage unassigned. */
export function useUnmarkAiProjectKeyPersonal(
  options?: DefaultMutationOptions<PersonalAiProjectKeyVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["ai-usage", "project-mappings", "personal", "unmark"],
    mutationFn: async (vars: PersonalAiProjectKeyVars) =>
      api.delete("ai-usage/project-mappings/personal", { json: vars }),
    ...options,
    onSettled: async (data, error, vars, ctx, mutationContext) => {
      await invalidateAiQueries(queryClient);
      await options?.onSettled?.(data, error, vars, ctx, mutationContext);
    },
  });
}
