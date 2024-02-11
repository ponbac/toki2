import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const pullRequestsQueries = {
  cachedPullRequests: () =>
    queryOptions({
      queryKey: ["cachedPullRequests"],
      queryFn: async () =>
        api.get("pull-requests/cached").json<Array<PullRequest>>(),
    }),
};

export type PullRequest = {
  organization: string;
  project: string;
  repoName: string;
  id: number;
  title: string;
  description: null | string;
  sourceBranch: string;
  targetBranch: string;
  status: Status;
  createdBy: CreatedBy;
  createdAt: Date;
  closedAt: null;
  autoCompleteSetBy: CreatedBy | null;
  completionOptions: CompletionOptions | null;
  isDraft: boolean;
  mergeStatus: MergeStatus | null;
  mergeJobId: string | null;
  mergeFailureType: MergeFailureType | null;
  mergeFailureMessage: string | null;
  reviewers: Reviewer[];
  url: string;
  threads: Thread[];
  commits: Commit[];
  workItems: WorkItem[];
};

export type CreatedBy = {
  id: string;
  displayName: string;
  uniqueName: string;
  avatarUrl: string;
};

export type Commit = {
  author: Author;
  comment: string;
  commitId: string;
  committer: Author;
  url: string;
};

export type Author = {
  date: Date;
  email: string;
  name: string;
};

export type CompletionOptions = {
  deleteSourceBranch: boolean;
  mergeCommitMessage: string;
  mergeStrategy: string;
  autoCompleteIgnoreConfigIds?: number[];
};

export type MergeStatus =
  | "succeeded"
  | "notSet"
  | "queued"
  | "conflicts"
  | "rejectedByPolicy"
  | "failure";

export type MergeFailureType =
  | "none"
  | "unknown"
  | "caseSensitive"
  | "objectTooLarge";

export type Reviewer = {
  identity: CreatedBy;
  vote: Vote;
  hasDeclined: boolean;
  isRequired: boolean | null;
  isFlagged: boolean;
};

export type Vote =
  | "NoResponse"
  | "Approved"
  | "ApprovedWithSuggestions"
  | "WaitingForAuthor"
  | "Rejected";

export type Status = "active" | "abandoned" | "completed" | "all" | "notSet";

export type Thread = {
  id: number;
  comments: Comment[];
  status: null | string;
  isDeleted: boolean;
  lastUpdatedAt: Date;
  publishedAt: Date;
};

export type Comment = {
  id: number;
  author: CreatedBy;
  content: string;
  commentType: CommentType;
  isDeleted: null;
  publishedAt: Date;
};

export type CommentType = "system" | "text" | "codeChange" | "unknown";

export type WorkItem = {
  id: number;
  parentId: number | null;
  title: string;
  state: string;
  itemType: string;
  createdAt: Date;
  changedAt: Date;
  assignedTo: CreatedBy;
  createdBy: CreatedBy;
  relations: Relation[];
};

export type Relation = {
  id: number | null;
  name: string;
  relationType: string;
  url: string;
};
