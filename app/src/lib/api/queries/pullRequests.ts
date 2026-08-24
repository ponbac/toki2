import { queryOptions } from "@tanstack/react-query";
import { parseListPullRequests } from "../contracts/pull-request";
import { api } from "../api";

export const pullRequestsQueries = {
  baseKey: ["pullRequests"],
  listPullRequests: () =>
    queryOptions({
      queryKey: [...pullRequestsQueries.baseKey, "listPullRequests"],
      queryFn: async () =>
        parseListPullRequests(
          await api.get("pull-requests/list").json<unknown>(),
        ),
    }),
};
