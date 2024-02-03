import { differsQueries } from "./differs";
import { pullRequestsQueries } from "./pullRequests";
import { commitsQueries } from "./commits";

export const queries = {
  ...differsQueries,
  ...pullRequestsQueries,
  ...commitsQueries,
};

export type RepoKey = {
  organization: string;
  project: string;
  repoName: string;
};
