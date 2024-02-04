import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import { RepoKey } from "./queries";

export const commitsQueries = {
  mostRecentCommits: (repoKey: RepoKey) =>
    queryOptions({
      queryKey: ["mostRecentCommits", repoKey],
      queryFn: async () =>
        api
          .get("pull-requests/most-recent-commits", { searchParams: repoKey })
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          .json<Array<any>>(),
    }),
};
