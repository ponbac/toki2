import { Schema } from "effect";
import { AgentRunStatuses } from "./status";

export const SourceSchema = Schema.Struct({
  type: Schema.Literals(["adoWorkItem", "githubIssue"]),
  id: Schema.String,
  title: Schema.String,
  url: Schema.String,
  markdown: Schema.String,
});
export type Source = typeof SourceSchema.Type;

export const TargetRepoSchema = Schema.Struct({
  provider: Schema.Literals(["azureDevOps", "github"]),
  cloneUrl: Schema.String,
  defaultBranch: Schema.String,
  organization: Schema.optionalKey(Schema.String),
  project: Schema.optionalKey(Schema.String),
  repoName: Schema.optionalKey(Schema.String),
  owner: Schema.optionalKey(Schema.String),
  repo: Schema.optionalKey(Schema.String),
});
export type TargetRepo = typeof TargetRepoSchema.Type;

const CreateTargetRepoSchema = Schema.Struct({
  provider: Schema.Literals(["azureDevOps", "github"]),
  cloneUrl: Schema.String,
  defaultBranch: Schema.String,
  organization: Schema.optionalKey(Schema.String),
  project: Schema.optionalKey(Schema.String),
  repoName: Schema.optionalKey(Schema.String),
  owner: Schema.optionalKey(Schema.String),
  repo: Schema.optionalKey(Schema.String),
  gitAuthHeader: Schema.optionalKey(Schema.String),
});
export type CreateTargetRepo = typeof CreateTargetRepoSchema.Type;

export const ActorSchema = Schema.Struct({
  tokiUserId: Schema.Number,
  displayName: Schema.String,
});
export type Actor = typeof ActorSchema.Type;

export const CreateAgentRunRequestSchema = Schema.Struct({
  mode: Schema.Literal("planFirst"),
  source: SourceSchema,
  targetRepo: CreateTargetRepoSchema,
  actor: ActorSchema,
  prompt: Schema.optionalKey(Schema.String),
});
export type CreateAgentRunRequest = typeof CreateAgentRunRequestSchema.Type;

export const AgentRunEventSchema = Schema.Struct({
  id: Schema.String,
  runId: Schema.String,
  status: Schema.Literals(AgentRunStatuses),
  message: Schema.String,
  createdAt: Schema.String,
});
export type AgentRunEvent = typeof AgentRunEventSchema.Type;

export const FeedbackRequestSchema = Schema.Struct({
  message: Schema.String,
  actor: ActorSchema,
});
export type FeedbackRequest = typeof FeedbackRequestSchema.Type;

export const PublishChangeSchema = Schema.Struct({
  changeType: Schema.Literals(["add", "edit", "delete"]),
  path: Schema.String,
  content: Schema.optionalKey(Schema.String),
});
export type PublishChange = typeof PublishChangeSchema.Type;

export const BackendPublishPayloadSchema = Schema.Struct({
  branchName: Schema.String,
  baseObjectId: Schema.String,
  title: Schema.String,
  description: Schema.String,
  changes: Schema.Array(PublishChangeSchema),
});
export type BackendPublishPayload = typeof BackendPublishPayloadSchema.Type;

export const CompletePublishRequestSchema = Schema.Struct({
  draftPrUrl: Schema.String,
});
export type CompletePublishRequest = typeof CompletePublishRequestSchema.Type;

export const FailPublishRequestSchema = Schema.Struct({
  message: Schema.String,
});
export type FailPublishRequest = typeof FailPublishRequestSchema.Type;

export const AgentRunWorkpadSchema = Schema.Struct({
  currentPlanMarkdown: Schema.String,
  planVersion: Schema.Number,
  feedbackHistory: Schema.Array(
    Schema.Struct({
      id: Schema.String,
      message: Schema.String,
      createdAt: Schema.String,
      actor: ActorSchema,
    }),
  ),
  acceptanceCriteria: Schema.Array(Schema.String),
  validationChecklist: Schema.Array(Schema.String),
  notes: Schema.Array(Schema.String),
  risksAndConfusions: Schema.Array(Schema.String),
  finalSummary: Schema.optionalKey(Schema.String),
  draftPrUrl: Schema.optionalKey(Schema.String),
});
export type AgentRunWorkpad = typeof AgentRunWorkpadSchema.Type;

export const AgentRunMetadataSchema = Schema.Struct({
  model: Schema.optionalKey(Schema.String),
  reasoningLevel: Schema.optionalKey(Schema.String),
  tokenUsage: Schema.optionalKey(
    Schema.Struct({
      inputTokens: Schema.optionalKey(Schema.Number),
      outputTokens: Schema.optionalKey(Schema.Number),
      totalTokens: Schema.optionalKey(Schema.Number),
    }),
  ),
});
export type AgentRunMetadata = typeof AgentRunMetadataSchema.Type;

const AgentWorkflowSnapshotSchema = Schema.Struct({
  source: Schema.Literals(["repo", "default"]),
  config: Schema.Struct({
    agent: Schema.Struct({
      harness: Schema.Literal("opencode"),
      maxTurns: Schema.Number,
    }),
    sandbox: Schema.Struct({
      workspaceDir: Schema.String,
    }),
    setup: Schema.Struct({
      commands: Schema.Array(Schema.String),
    }),
    verify: Schema.Struct({
      commands: Schema.Array(Schema.String),
    }),
    publish: Schema.Struct({
      draftPr: Schema.Boolean,
      branchPattern: Schema.String,
    }),
  }),
  promptPolicy: Schema.String,
});

export const AgentRunRecordSchema = Schema.Struct({
  id: Schema.String,
  status: Schema.Literals(AgentRunStatuses),
  mode: Schema.Literal("planFirst"),
  source: SourceSchema,
  targetRepo: TargetRepoSchema,
  actor: ActorSchema,
  prompt: Schema.optionalKey(Schema.String),
  metadata: Schema.optionalKey(AgentRunMetadataSchema),
  workflowSnapshot: Schema.optionalKey(AgentWorkflowSnapshotSchema),
  pendingPublish: Schema.optionalKey(BackendPublishPayloadSchema),
  workpad: AgentRunWorkpadSchema,
  events: Schema.Array(AgentRunEventSchema),
  createdAt: Schema.String,
  updatedAt: Schema.String,
});
export type AgentRunRecord = typeof AgentRunRecordSchema.Type;
