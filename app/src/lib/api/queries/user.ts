import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import { parseApiTokens, parseUser } from "../contracts/user";

export type {
  ApiToken,
  ApiTokenCapability,
  ApiTokenCapabilities,
  CreatedApiToken,
  Role,
  User,
} from "../contracts/user";
export { API_TOKEN_CAPABILITIES } from "../contracts/user";

const userQueryKeys = {
  profile: ["user", "profile"] as const,
  apiTokens: ["user", "api-tokens"] as const,
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
};
