import { differsQueries } from "./differs";
import { pullRequestsQueries } from "./pullRequests";
import { commitsQueries } from "./commits";
import { milltimeQueries } from "./milltime";

export const queries = {
  ...differsQueries,
  ...pullRequestsQueries,
  ...commitsQueries,
  ...milltimeQueries,
};

export type RepoKey<T = object> = T & {
  organization: string;
  project: string;
  repoName: string;
};
