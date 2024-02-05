import { useMutation, useQueryClient } from "@tanstack/react-query";
import { RepoKey, queries } from "../queries/queries";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";

export const differsMutations = { useStartDiffers, useStopDiffers };

function useStartDiffers(options?: DefaultMutationOptions<RepoKey>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["differs", "start"],
    mutationFn: (repoKey: RepoKey) =>
      api.post("differs/start", {
        json: repoKey,
      }),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      queryClient.invalidateQueries(queries.cachedPullRequests());
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useStopDiffers(options?: DefaultMutationOptions<RepoKey>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["differs", "stop"],
    mutationFn: (repoKey: RepoKey) =>
      api.post("differs/stop", {
        json: repoKey,
      }),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}
