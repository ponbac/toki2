import {
  useMutation,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { api } from "../api";
import { parseCreatedApiToken } from "../contracts/user";
import { pullRequestsQueries } from "../queries/pullRequests";
import { workItemsQueries } from "../queries/workItems";
import { userQueries, type CreatedApiToken } from "../queries/user";
import type { DefaultMutationOptions } from "./mutations";

/** Input for uploading the current user's avatar. */
export type UploadAvatarVars = { file: File };
/** Input for issuing a session-equivalent personal API token. */
export type CreateApiTokenVars = { name: string };
/** Input for revoking one of the current user's API tokens. */
export type RevokeApiTokenVars = { tokenId: number };

/** Mutation hooks for current-user profile and credential operations. */
export const userMutations = {
  useUploadAvatar,
  useDeleteAvatar,
  useCreateApiToken,
  useRevokeApiToken,
};

/** Uploads an avatar and refreshes every view that renders user identities. */
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
    onSuccess: async (data, vars, ctx) => {
      await invalidateUserIdentityQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/** Deletes the avatar override and refreshes rendered user identities. */
export function useDeleteAvatar(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["user", "avatar", "delete"],
    mutationFn: async () => api.delete("users/me/avatar"),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await invalidateUserIdentityQueries(queryClient);
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

async function invalidateUserIdentityQueries(queryClient: QueryClient) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: userQueries.me().queryKey }),
    queryClient.invalidateQueries({ queryKey: pullRequestsQueries.baseKey }),
    queryClient.invalidateQueries({ queryKey: workItemsQueries.baseKey }),
  ]);
}

/** Issues a session-equivalent personal API token and refreshes token metadata. */
export function useCreateApiToken(
  options?: DefaultMutationOptions<CreateApiTokenVars, CreatedApiToken>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["user", "api-tokens", "create"],
    mutationFn: async ({ name }: CreateApiTokenVars) =>
      parseCreatedApiToken(
        await api
          .post("users/me/api-tokens", { json: { name } })
          .json<unknown>(),
      ),
    ...options,
    // Clear detached secret-bearing results promptly without a zero-delay GC
    // loop if the observer disappears while the request is still pending.
    gcTime: 1_000,
    onSuccess: async (data, vars, ctx) => {
      // Surface the one-time secret before waiting on a metadata refetch.
      await options?.onSuccess?.(data, vars, ctx);
      await queryClient.invalidateQueries({
        queryKey: userQueries.apiTokens().queryKey,
      });
    },
  });
}

/** Revokes one API token and refreshes token metadata. */
export function useRevokeApiToken(
  options?: DefaultMutationOptions<RevokeApiTokenVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["user", "api-tokens", "revoke"],
    mutationFn: async ({ tokenId }: RevokeApiTokenVars) =>
      api.delete(`users/me/api-tokens/${tokenId}`),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      await queryClient.invalidateQueries({
        queryKey: userQueries.apiTokens().queryKey,
      });
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}
