import { differsQueries } from "./differs";
import { pullRequestsQueries } from "./pullRequests";
import { commitsQueries } from "./commits";
import { timeTrackingQueries } from "./time-tracking";
import { userQueries } from "./user";

export const queries = {
  ...userQueries,
  ...differsQueries,
  ...pullRequestsQueries,
  ...commitsQueries,
  ...timeTrackingQueries,
};

export type RepoKey<T = object> = T & {
  organization: string;
  project: string;
  repoName: string;
};
