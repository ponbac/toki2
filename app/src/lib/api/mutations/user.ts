import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
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
      return api.post("me/avatar", { body: formData });
    },
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: ["me"] });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export function useDeleteAvatar(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["user", "avatar", "delete"],
    mutationFn: async () => api.delete("me/avatar"),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: ["me"] });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}
