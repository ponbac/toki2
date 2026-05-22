import type { AgentRun } from "./runs/AgentRun";
import type { Sandbox as CloudflareSandbox } from "@cloudflare/sandbox";
import type { AgentRunWorkflowParams } from "./workflow/agent-run-workflow-types";

export type Env = {
  readonly TOKI_AGENT_INTERNAL_TOKEN?: string;
  readonly OPENCODE_AUTH_JSON?: string;
  readonly OPENCODE_MODEL?: string;
  readonly OPENCODE_VARIANT?: string;
  readonly OPENCODE_ALLOW_OPENAI_API_KEY?: string;
  readonly OPENAI_API_KEY?: string;
  readonly GEMINI_API_KEY?: string;
  readonly AGENT_RUN?: DurableObjectNamespace<AgentRun>;
  readonly AGENT_RUN_WORKFLOW?: Workflow<AgentRunWorkflowParams>;
  readonly SANDBOX_V2?: DurableObjectNamespace<CloudflareSandbox>;
};
