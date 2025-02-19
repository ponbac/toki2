import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const differsQueries = {
  differs: () =>
    queryOptions({
      queryKey: ["differs"],
      queryFn: async () => api.get("differs").json<Array<Differ>>(),
    }),
};

export type Differ = {
  organization: string;
  project: string;
  repoId: number;
  repoName: string;
  followed: boolean;
  status: "Running" | "Stopped" | "Errored";
  lastUpdated: string | null;
  refreshInterval: {
    secs: number;
    nanos: number;
  } | null;
  isInvalid: boolean;
};
