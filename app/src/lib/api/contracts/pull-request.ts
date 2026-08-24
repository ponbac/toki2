import { z } from "zod";

const pullRequestIdentitySchema = z
  .object({
    id: z.string(),
    displayName: z.string(),
    uniqueName: z.string(),
    avatarUrl: z.string().url().nullable(),
  })
  .strict();

const pullRequestVoteSchema = z.enum([
  "NoResponse",
  "Approved",
  "ApprovedWithSuggestions",
  "WaitingForAuthor",
  "Rejected",
]);

const pullRequestReviewerSchema = z
  .object({
    identity: pullRequestIdentitySchema,
    vote: pullRequestVoteSchema.nullable(),
    hasDeclined: z.boolean().nullable(),
    isRequired: z.boolean().nullable(),
    isFlagged: z.boolean().nullable(),
  })
  .strict();

const pullRequestCommentSchema = z
  .object({
    id: z.number().int(),
    author: pullRequestIdentitySchema,
    content: z.string().nullable(),
    commentType: z.enum(["unknown", "text", "codeChange", "system"]).nullable(),
    isDeleted: z.boolean().nullable(),
    publishedAt: z.string().datetime({ offset: true }),
  })
  .strict();

const pullRequestThreadSchema = z
  .object({
    id: z.number().int(),
    comments: z.array(pullRequestCommentSchema),
    status: z
      .enum([
        "unknown",
        "active",
        "fixed",
        "wontFix",
        "closed",
        "byDesign",
        "pending",
      ])
      .nullable(),
    isDeleted: z.boolean().nullable(),
    lastUpdatedAt: z.string().datetime({ offset: true }),
    publishedAt: z.string().datetime({ offset: true }),
  })
  .strict();

const pullRequestWorkItemSchema = z
  .object({
    id: z.string(),
    title: z.string(),
    url: z.string().url(),
    parentId: z.string().nullable(),
    priority: z.number().int().nullable(),
  })
  .strict();

const listPullRequestSchema = z
  .object({
    organization: z.string(),
    project: z.string(),
    repoName: z.string(),
    url: z.string().url(),
    id: z.number().int(),
    title: z.string(),
    createdBy: pullRequestIdentitySchema,
    createdAt: z.string().datetime({ offset: true }),
    sourceBranch: z.string(),
    targetBranch: z.string(),
    isDraft: z.boolean(),
    mergeStatus: z
      .enum([
        "notSet",
        "queued",
        "conflicts",
        "succeeded",
        "rejectedByPolicy",
        "failure",
      ])
      .nullable(),
    threads: z.array(pullRequestThreadSchema),
    workItems: z.array(pullRequestWorkItemSchema),
    reviewers: z.array(pullRequestReviewerSchema),
    blockedBy: z.array(pullRequestReviewerSchema),
    approvedBy: z.array(pullRequestReviewerSchema),
    waitingForUserReview: z.boolean(),
    reviewRequired: z.boolean(),
  })
  .strict();

/** A provider-neutral identity attached to a pull request. */
export type PullRequestIdentity = Readonly<
  z.infer<typeof pullRequestIdentitySchema>
>;

/** A discussion thread attached to a pull request. */
export type PullRequestThread = Readonly<
  z.infer<typeof pullRequestThreadSchema>
>;

/** The trimmed pull-request representation returned by the list endpoint. */
export type ListPullRequest = Readonly<z.infer<typeof listPullRequestSchema>>;

/** Parses pull-request list data at the HTTP boundary. */
export function parseListPullRequests(input: unknown): ListPullRequest[] {
  return z.array(listPullRequestSchema).parse(input);
}
