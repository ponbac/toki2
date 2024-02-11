import { useMutation, useQueryClient } from "@tanstack/react-query";
import { RepoKey, queries } from "../queries/queries";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { Differ } from "../queries/differs";

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
    onMutate: (repoKey) => {
      queryClient.cancelQueries(queries.differs());
      const previous = queryClient.getQueryData(queries.differs().queryKey);

      queryClient.setQueryData(queries.differs().queryKey, (old) => {
        if (!old) return undefined;

        const differ = old.find(
          (differ) =>
            differ.organization === repoKey.organization &&
            differ.project === repoKey.project &&
            differ.repoName === repoKey.repoName,
        );
        if (!differ) return old;

        return old.map((d) =>
          d === differ
            ? {
                ...d,
                status: "Running" as Differ["status"],
              }
            : d,
        );
      });

      return { previous };
    },
    onError: (err, vars, ctx) => {
      queryClient.setQueryData(queries.differs().queryKey, ctx?.previous);
      options?.onError?.(err, vars, ctx);
    },
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
    onMutate: (repoKey) => {
      queryClient.cancelQueries(queries.differs());
      const previous = queryClient.getQueryData(queries.differs().queryKey);

      queryClient.setQueryData(queries.differs().queryKey, (old) => {
        if (!old) return undefined;

        const differ = old.find(
          (differ) =>
            differ.organization === repoKey.organization &&
            differ.project === repoKey.project &&
            differ.repoName === repoKey.repoName,
        );
        if (!differ) return old;

        return old.map((d) =>
          d === differ
            ? {
                ...d,
                status: "Stopped" as Differ["status"],
              }
            : d,
        );
      });

      return { previous };
    },
    onError: (err, vars, ctx) => {
      queryClient.setQueryData(queries.differs().queryKey, ctx?.previous);
      options?.onError?.(err, vars, ctx);
    },
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}
