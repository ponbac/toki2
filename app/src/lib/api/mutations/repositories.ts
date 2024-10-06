import { useMutation, useQueryClient } from "@tanstack/react-query";
import { RepoKey, queries } from "../queries/queries";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";

export const repositoriesMutations = {
  useAddRepository,
  useFollowRepository,
  useDeleteRepository,
};

function useAddRepository(options?: DefaultMutationOptions<AddRepositoryBody>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["addRepository"],
    mutationFn: (body: AddRepositoryBody) =>
      api.post("repositories", {
        json: body,
      }),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

type FollowRepositoryBody = RepoKey & { follow: boolean };

function useFollowRepository(
  options?: DefaultMutationOptions<FollowRepositoryBody>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["followRepository"],
    mutationFn: (body: FollowRepositoryBody) =>
      api.post("repositories/follow", {
        json: body,
      }),
    ...options,
    onMutate: (vars) => {
      queryClient.setQueryData(queries.differs().queryKey, (old) => {
        if (!old) return old;

        const idx = old.findIndex(
          (d) =>
            d.organization === vars.organization &&
            d.project === vars.project &&
            d.repoName === vars.repoName,
        );
        if (idx === -1) return old;
        return [
          ...old.slice(0, idx),
          { ...old[idx], followed: vars.follow },
          ...old.slice(idx + 1),
        ];
      });
      options?.onMutate?.(vars);
    },
    onSettled: (data, err, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      queryClient.invalidateQueries(queries.cachedPullRequests());
      options?.onSettled?.(data, err, vars, ctx);
    },
  });
}

function useDeleteRepository(
  options?: DefaultMutationOptions<DeleteRepositoryBody>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["deleteRepository"],
    mutationFn: (body: DeleteRepositoryBody) =>
      api.delete("repositories", {
        json: body,
      }),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      queryClient.invalidateQueries(queries.cachedPullRequests());
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export const addRepositorySchema = z.object({
  organization: z.string().min(1, "Organization is required"),
  project: z.string().min(1, "Project is required"),
  repoName: z.string().min(1, "Repository name is required"),
  token: z.string().min(1, "Token is required"),
});

export type AddRepositoryBody = z.infer<typeof addRepositorySchema>;

export type DeleteRepositoryBody = {
  organization: string;
  project: string;
  repoName: string;
};
