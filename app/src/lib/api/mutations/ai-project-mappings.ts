import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { parseAiProjectMapping } from "../contracts/ai-usage-report";
import {
  aiUsageReportQueryKeys,
  type AiProjectMapping,
} from "../queries/ai-usage-report";
import type { DefaultMutationOptions } from "./mutations";

/** Input for mapping an unmapped project key in the user's own usage. */
export type MapAiProjectKeyVars = Readonly<{
  projectKey: string;
  projectId: string;
}>;

/** Mutation hooks for project mappings made from the personal usage view. */
export const aiProjectMappingMutations = {
  useMapAiProjectKey,
};

/**
 * Maps an unmapped key in the user's usage to a time-tracking project. The
 * server refuses keys that are already mapped (changing one takes an admin)
 * and keys outside the user's usage. Mappings apply retroactively, so every
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
    onSettled: async (data, error, vars, ctx) => {
      await queryClient.invalidateQueries({
        queryKey: aiUsageReportQueryKeys.all,
      });
      await options?.onSettled?.(data, error, vars, ctx);
    },
  });
}
