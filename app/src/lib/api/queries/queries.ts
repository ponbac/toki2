import { aiUsageQueries } from "./ai-usage";
import { aiUsageReportQueries } from "./ai-usage-report";
import { differsQueries } from "./differs";
import { pullRequestsQueries } from "./pullRequests";
import { commitsQueries } from "./commits";
import { timeTrackingQueries } from "./time-tracking";
import { userQueries } from "./user";
import { workItemsQueries } from "./workItems";

export const queries = {
  ...aiUsageQueries,
  ...aiUsageReportQueries,
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
