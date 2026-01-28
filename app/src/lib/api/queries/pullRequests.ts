import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const pullRequestsQueries = {
  baseKey: ["pullRequests"],
  cachedPullRequests: () =>
    queryOptions({
      queryKey: [...pullRequestsQueries.baseKey, "cachedPullRequests"],
      queryFn: async () =>
        api.get("pull-requests/cached").json<Array<PullRequest>>(),
      refetchInterval: 60 * 1000,
    }),
  listPullRequests: () =>
    queryOptions({
      queryKey: [...pullRequestsQueries.baseKey, "listPullRequests"],
      queryFn: async () =>
        api.get("pull-requests/list").json<Array<ListPullRequest>>(),
    }),
};

export type ListPullRequest = {
  organization: string;
  project: string;
  repoName: string;
  id: number;
  title: string;
  createdBy: User;
  createdAt: string;
  sourceBranch: string;
  targetBranch: string;
  isDraft: boolean;
  mergeStatus: MergeStatus | null;
  threads: Thread[];
  workItems: WorkItem[];
  reviewers: Reviewer[];
  blockedBy: Reviewer[];
  approvedBy: Reviewer[];
  waitingForUserReview: boolean;
  reviewRequired: boolean;
  avatarOverrides: AvatarOverride[];
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
  createdBy: User;
  createdAt: Date;
  closedAt: null;
  autoCompleteSetBy: User | null;
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
  blockedBy: Reviewer[];
};

export type User = {
  id: string;
  displayName: string;
  uniqueName: string;
  avatarUrl: string;
};

export type AvatarOverride = {
  email: string;
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
  identity: User;
  vote: Vote | null;
  hasDeclined: boolean | null;
  isRequired: boolean | null;
  isFlagged: boolean | null;
};

export type Vote =
  | "NoResponse"
  | "Approved"
  | "ApprovedWithSuggestions"
  | "WaitingForAuthor"
  | "Rejected";

export type Status = "active" | "abandoned" | "completed" | "all" | "notSet";

export type ThreadStatus =
  | "unknown"
  | "active"
  | "fixed"
  | "wontFix"
  | "closed"
  | "byDesign"
  | "pending";

export type Thread = {
  id: number;
  comments: Comment[];
  status: ThreadStatus | null;
  isDeleted: boolean;
  lastUpdatedAt: Date;
  publishedAt: Date;
};

export type Comment = {
  id: number;
  author: User;
  content: string;
  commentType: CommentType | null;
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
  priority?: number | null;
  createdAt: Date;
  changedAt: Date;
  assignedTo: User;
  createdBy: User;
  relations: Relation[];
};

export type Relation = {
  id: number | null;
  name: string;
  relationType: string;
  url: string;
};
