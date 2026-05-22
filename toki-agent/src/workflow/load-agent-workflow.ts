import { Option, pipe, Result, Schema } from "effect";
import YAML from "yaml";

export type AgentWorkflowConfig = {
  readonly agent: {
    readonly harness: "opencode";
    readonly maxTurns: number;
  };
  readonly sandbox: {
    readonly workspaceDir: string;
  };
  readonly setup: {
    readonly commands: ReadonlyArray<string>;
  };
  readonly verify: {
    readonly commands: ReadonlyArray<string>;
  };
  readonly publish: {
    readonly draftPr: boolean;
    readonly branchPattern: string;
  };
};

export type AgentWorkflow = {
  readonly config: AgentWorkflowConfig;
  readonly promptPolicy: string;
  readonly source: "repo" | "default";
};

const DEFAULT_PROMPT_POLICY = `You are working on a Toki board item.

Issue context:
{{ issue.markdown }}

Plan-first rules:
- First produce an implementation plan.
- Do not edit files while planning.
- Include likely files, risks, verification strategy, and open questions.
- When feedback is provided, revise the plan instead of implementing.
- Only implement after explicit approval.

Implementation rules:
- Implement the approved plan.
- Make the smallest safe change.
- Follow repository conventions.
- Use static code inspection when browser/prod access is unavailable; do not block on manual profiling.
- Run the configured verification commands.
- Create a draft PR with a clear summary and validation evidence.`;

export const DEFAULT_AGENT_WORKFLOW_CONFIG: AgentWorkflowConfig = {
  agent: {
    harness: "opencode",
    maxTurns: 4,
  },
  sandbox: {
    workspaceDir: "/workspace/repo",
  },
  setup: {
    commands: [],
  },
  verify: {
    commands: ["just check", "just tsc", "just lint"],
  },
  publish: {
    draftPr: true,
    branchPattern: "agent/{sourceType}-{sourceId}-{slug}",
  },
};

export const DEFAULT_AGENT_WORKFLOW: AgentWorkflow = {
  config: DEFAULT_AGENT_WORKFLOW_CONFIG,
  promptPolicy: DEFAULT_PROMPT_POLICY,
  source: "default",
};

const CommandListSchema = Schema.Struct({
  commands: Schema.optionalKey(Schema.Array(Schema.String)),
});

const AgentWorkflowFrontMatterSchema = Schema.Struct({
  agent: Schema.optionalKey(
    Schema.Struct({
      harness: Schema.optionalKey(Schema.Literal("opencode")),
      max_turns: Schema.optionalKey(Schema.Number),
    }),
  ),
  sandbox: Schema.optionalKey(
    Schema.Struct({
      workspace_dir: Schema.optionalKey(Schema.String),
    }),
  ),
  setup: Schema.optionalKey(CommandListSchema),
  verify: Schema.optionalKey(CommandListSchema),
  publish: Schema.optionalKey(
    Schema.Struct({
      draft_pr: Schema.optionalKey(Schema.Boolean),
      branch_pattern: Schema.optionalKey(Schema.String),
    }),
  ),
});

type AgentWorkflowFrontMatter = typeof AgentWorkflowFrontMatterSchema.Type;

const FRONT_MATTER_PATTERN = /^---\n([\s\S]*?)\n---\s*\n?([\s\S]*)$/;

const splitFrontMatter = (content: string): { readonly yaml: string; readonly body: string } | undefined =>
  pipe(
    Option.fromNullishOr(content.match(FRONT_MATTER_PATTERN)),
    Option.match({
      onNone: () => undefined,
      onSome: (match) => ({
        yaml: match[1] ?? "",
        body: match[2]?.trimStart() ?? "",
      }),
    }),
  );

const mergeConfig = (frontMatter: AgentWorkflowFrontMatter): AgentWorkflowConfig => ({
  agent: {
    harness: frontMatter.agent?.harness ?? DEFAULT_AGENT_WORKFLOW_CONFIG.agent.harness,
    maxTurns: frontMatter.agent?.max_turns ?? DEFAULT_AGENT_WORKFLOW_CONFIG.agent.maxTurns,
  },
  sandbox: {
    workspaceDir: frontMatter.sandbox?.workspace_dir ?? DEFAULT_AGENT_WORKFLOW_CONFIG.sandbox.workspaceDir,
  },
  setup: {
    commands: frontMatter.setup?.commands ?? DEFAULT_AGENT_WORKFLOW_CONFIG.setup.commands,
  },
  verify: {
    commands: frontMatter.verify?.commands ?? DEFAULT_AGENT_WORKFLOW_CONFIG.verify.commands,
  },
  publish: {
    draftPr: frontMatter.publish?.draft_pr ?? DEFAULT_AGENT_WORKFLOW_CONFIG.publish.draftPr,
    branchPattern: frontMatter.publish?.branch_pattern ?? DEFAULT_AGENT_WORKFLOW_CONFIG.publish.branchPattern,
  },
});

const parseNonEmptyAgentWorkflow = (content: string): AgentWorkflow =>
  pipe(
    Option.fromNullishOr(splitFrontMatter(content)),
    Option.match({
      onNone: () => ({
        ...DEFAULT_AGENT_WORKFLOW,
        promptPolicy: content.trim(),
        source: "repo" as const,
      }),
      onSome: parseFrontMatterWorkflow,
    }),
  );

const parseFrontMatterWorkflow = (split: { readonly yaml: string; readonly body: string }): AgentWorkflow =>
  pipe(
    Schema.decodeUnknownResult(AgentWorkflowFrontMatterSchema)(YAML.parse(split.yaml) as unknown),
    Result.match({
      onFailure: () => ({
        ...DEFAULT_AGENT_WORKFLOW,
        promptPolicy: split.body.trim() || DEFAULT_PROMPT_POLICY,
        source: "repo" as const,
      }),
      onSuccess: (frontMatter) => ({
        config: mergeConfig(frontMatter),
        promptPolicy: split.body.trim() || DEFAULT_PROMPT_POLICY,
        source: "repo" as const,
      }),
    }),
  );

export const parseAgentWorkflow = (content: string | undefined): AgentWorkflow =>
  pipe(
    Option.fromNullishOr(content),
    Option.map((value) => value.trim()),
    Option.filter((value) => value.length > 0),
    Option.match({
      onNone: () => DEFAULT_AGENT_WORKFLOW,
      onSome: parseNonEmptyAgentWorkflow,
    }),
  );
