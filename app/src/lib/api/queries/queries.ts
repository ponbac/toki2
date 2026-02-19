import { differsQueries } from "./differs";
import { pullRequestsQueries } from "./pullRequests";
import { commitsQueries } from "./commits";
import { timeTrackingQueries } from "./time-tracking";
import { userQueries } from "./user";
import { workItemsQueries } from "./workItems";

export const queries = {
  ...userQueries,
  ...differsQueries,
  ...pullRequestsQueries,
  ...commitsQueries,
  ...timeTrackingQueries,
  ...workItemsQueries,
};

export type RepoKey<T = object> = T & {
  organization: string;
  project: string;
  repoName: string;
};
