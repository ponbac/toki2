import { differsQueries } from "./differs";
import { pullRequestsQueries } from "./pullRequests";
import { commitsQueries } from "./commits";
import { milltimeQueries } from "./milltime";
import { userQueries } from "./user";
import { searchQueries } from "./search";

export const queries = {
  user: userQueries,
  differs: differsQueries,
  pullRequests: pullRequestsQueries,
  commits: commitsQueries,
  milltime: milltimeQueries,
  search: searchQueries,
};

export type RepoKey<T = object> = T & {
  organization: string;
  project: string;
  repoName: string;
};
