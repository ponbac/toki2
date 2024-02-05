import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const pullRequestsQueries = {
  cachedPullRequests: () =>
    queryOptions({
      queryKey: ["cachedPullRequests"],
      queryFn: async () =>
        api
          .get("pull-requests/cached")
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          .json<Array<any>>(),
    }),
};
