import { queryOptions } from "@tanstack/react-query";
import { api } from "./api";

export const queries = {
  differs: () =>
    queryOptions({
      queryKey: ["differs"],
      queryFn: async () => api.get("differs").json<Array<Differ>>(),
      refetchInterval: 30 * 1000,
    }),
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

export type RepoKey = {
  organization: string;
  project: string;
  repoName: string;
};

type Differ = {
  organization: string;
  project: string;
  repoName: string;
  status: "Running" | "Stopped";
  lastUpdated: string | null;
  refreshInterval: {
    secs: number;
    nanos: number;
  } | null;
};
