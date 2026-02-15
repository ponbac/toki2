import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { pullRequestsQueries } from "../queries/pullRequests";
import { workItemsQueries } from "../queries/workItems";
import type { DefaultMutationOptions } from "./mutations";

export type UploadAvatarVars = { file: File };

export const userMutations = {
  useUploadAvatar,
  useDeleteAvatar,
};

export function useUploadAvatar(
  options?: DefaultMutationOptions<UploadAvatarVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["user", "avatar", "upload"],
    mutationFn: async ({ file }: UploadAvatarVars) => {
      const formData = new FormData();
      formData.append("avatar", file);
      return api.post("users/me/avatar", { body: formData });
    },
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: ["me"] });
      queryClient.invalidateQueries({ queryKey: pullRequestsQueries.baseKey });
      queryClient.invalidateQueries({ queryKey: workItemsQueries.baseKey });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useDeleteAvatar(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["user", "avatar", "delete"],
    mutationFn: async () => api.delete("users/me/avatar"),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: ["me"] });
      queryClient.invalidateQueries({ queryKey: pullRequestsQueries.baseKey });
      queryClient.invalidateQueries({ queryKey: workItemsQueries.baseKey });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}
