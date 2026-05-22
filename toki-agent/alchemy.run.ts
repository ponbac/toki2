import * as Alchemy from "alchemy";
import * as Cloudflare from "alchemy/Cloudflare";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Output from "alchemy/Output";
import * as Redacted from "effect/Redacted";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { randomBytes } from "node:crypto";
import type { AgentRun } from "./src/runs/AgentRun";
import type { SandboxV2 } from "./src/sandbox/Sandbox";

const internalTokenFile = join(
  import.meta.dir,
  ".alchemy",
  "toki-agent-internal-token",
);

const readOrCreateInternalToken = () => {
  if (existsSync(internalTokenFile)) {
    return readFileSync(internalTokenFile, "utf8").trim();
  }

  mkdirSync(dirname(internalTokenFile), { recursive: true });
  const token = randomBytes(32).toString("base64url");
  writeFileSync(internalTokenFile, `${token}\n`, { mode: 0o600 });
  return token;
};

const localOpenCodeAuthFile = join(
  process.env.HOME ?? "",
  ".local",
  "share",
  "opencode",
  "auth.json",
);

const readOpenCodeAuthJson = () => {
  if (process.env.OPENCODE_AUTH_JSON !== undefined && process.env.OPENCODE_AUTH_JSON.trim().length > 0) {
    return process.env.OPENCODE_AUTH_JSON;
  }

  if (existsSync(localOpenCodeAuthFile)) {
    return readFileSync(localOpenCodeAuthFile, "utf8");
  }

  return undefined;
};

const opencodeModel = process.env.OPENCODE_MODEL ?? "openai/gpt-5.4";
const opencodeVariant = process.env.OPENCODE_VARIANT;
const allowOpenAiApiKey = process.env.OPENCODE_ALLOW_OPENAI_API_KEY === "1";
const openaiApiKey = allowOpenAiApiKey ? process.env.OPENAI_API_KEY : undefined;
const geminiApiKey = process.env.GEMINI_API_KEY;
const opencodeAuthJson =
  openaiApiKey === undefined || openaiApiKey.trim().length === 0
    ? readOpenCodeAuthJson()
    : undefined;

export const Worker = Cloudflare.Worker("TokiAgentWorker", {
  name: "toki-agent",
  main: "./src/worker.ts",
  compatibility: {
    date: "2026-04-07",
    flags: ["nodejs_compat"],
  },
  env: {
    TOKI_AGENT_INTERNAL_TOKEN: Redacted.make(readOrCreateInternalToken()),
    OPENCODE_MODEL: opencodeModel,
    ...(opencodeVariant === undefined || opencodeVariant.trim().length === 0
      ? {}
      : {
          OPENCODE_VARIANT: opencodeVariant,
        }),
    ...(openaiApiKey === undefined || openaiApiKey.trim().length === 0
      ? {}
      : {
          OPENAI_API_KEY: Redacted.make(openaiApiKey),
        }),
    ...(geminiApiKey === undefined || geminiApiKey.trim().length === 0
      ? {}
      : {
          GEMINI_API_KEY: Redacted.make(geminiApiKey),
        }),
    ...(opencodeAuthJson === undefined
      ? {}
      : {
          OPENCODE_AUTH_JSON: Redacted.make(opencodeAuthJson),
        }),
  },
  bindings: {
    AGENT_RUN: Cloudflare.DurableObjectNamespace<AgentRun>("AgentRun", {
      className: "AgentRun",
    }),
    SANDBOX_V2: Cloudflare.DurableObjectNamespace<SandboxV2>("SandboxV2", {
      className: "SandboxV2",
    }),
  },
});

const SandboxContainerV3 = Cloudflare.Container<unknown>()("SandboxContainerV3", {
  name: "toki-agent-sandbox-v3",
  main: "./src/sandbox/container-entry.ts",
  dockerfile: readFileSync(join(import.meta.dir, "Dockerfile"), "utf8"),
  ports: [{ name: "sandboxhttp", port: 3000 }],
  entrypoint: ["/container-server/sandbox"],
  instances: 1,
  maxInstances: 20,
  instanceType: "standard-2",
  observability: { logs: { enabled: true } },
});

export type WorkerEnv = Cloudflare.InferEnv<typeof Worker> & {
  readonly TOKI_AGENT_INTERNAL_TOKEN?: string;
};

export default Alchemy.Stack(
  "TokiAgent",
  {
    providers: Layer.mergeAll(Cloudflare.providers(), Cloudflare.WorkflowProvider()),
    state: Cloudflare.state(),
  },
  Effect.gen(function* () {
    const worker = yield* Worker;
    const sandboxContainer = yield* SandboxContainerV3;

    yield* Cloudflare.WorkflowResource("AgentRunWorkflow", {
      workflowName: "AgentRunWorkflow",
      className: "AgentRunWorkflow",
      scriptName: worker.workerName,
    });

    yield* worker.bind`AgentRunWorkflowBinding`({
      bindings: [
        {
          type: "workflow",
          name: "AGENT_RUN_WORKFLOW",
          workflowName: "AgentRunWorkflow",
          className: "AgentRunWorkflow",
        },
      ],
    });

    yield* worker.bind`SandboxContainerClass`({
      containers: [{ className: "SandboxV2" }],
    });

    yield* sandboxContainer.bind`SandboxDurableObject`({
      durableObjects: {
        namespaceId: Output.map(
          worker.durableObjectNamespaces,
          (namespaces) => namespaces.SandboxV2,
        ),
      },
    });

    return {
      url: worker.url,
      internalTokenFile,
    };
  }),
);
