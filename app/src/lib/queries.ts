import { queryOptions } from "@tanstack/react-query";
import { api } from "./api";

export const queries = {
  differs: () =>
    queryOptions({
      queryKey: ["differs"],
      queryFn: async () => api.get("differs").json<Array<Differ>>(),
      refetchInterval: 30 * 1000,
    }),
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
