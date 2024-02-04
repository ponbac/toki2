import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import { RepoKey } from "./queries";

export const pullRequestsQueries = {
  cachedPullRequests: (repoKey: RepoKey) =>
    queryOptions({
      queryKey: ["cachedPullRequests", repoKey],
      queryFn: async () =>
        api
          .get("pull-requests/cached", { searchParams: repoKey })
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          .json<Array<any>>(),
    }),
};
