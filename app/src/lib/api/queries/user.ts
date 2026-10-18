import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import { parseApiTokens, parseOAuthGrants, parseUser } from "../contracts/user";

export type {
  ApiToken,
  CreatedApiToken,
  OAuthGrant,
  Role,
  User,
} from "../contracts/user";

const userQueryKeys = {
  profile: ["user", "profile"] as const,
  apiTokens: ["user", "api-tokens"] as const,
  oauthGrants: ["user", "oauth-grants"] as const,
};

/** Query definitions for current-user profile and credential metadata. */
export const userQueries = {
  me: () =>
    queryOptions({
      queryKey: userQueryKeys.profile,
      queryFn: async () => parseUser(await api.get("me").json<unknown>()),
    }),
  apiTokens: () =>
    queryOptions({
      queryKey: userQueryKeys.apiTokens,
      queryFn: async () =>
        parseApiTokens(await api.get("users/me/api-tokens").json<unknown>()),
    }),
  oauthGrants: () =>
    queryOptions({
      queryKey: userQueryKeys.oauthGrants,
      queryFn: async () =>
        parseOAuthGrants(
          await api.get("users/me/oauth-grants").json<unknown>(),
        ),
    }),
};
