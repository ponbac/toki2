# Toki Agent

Cloudflare-only MVP service for board-managed coding-agent runs.

Current phase:

- Alchemy v2 project skeleton.
- Cloudflare Worker health route at `GET /health`.
- Internal run API with Durable Object-backed run state.
- Run reconciliation progresses through sandbox provisioning, repository clone, workflow loading, setup, planning, implementation, verification, and draft PR publishing.
- Cloudflare Workflows drive run orchestration. The Durable Object stores run state; the Workflow owns durable background progress and waits for plan approval or feedback with `waitForEvent`.
- OpenCode is invoked through the CLI's non-interactive `opencode run [message..]` command inside the sandbox for planning and implementation.
- Effect v4-compatible schemas and pure helpers for workflow parsing, prompt rendering, status transitions, branch naming, PR descriptions, and log redaction.
- Biome plus `@catenarycloud/linteffect` core rules for Effect-oriented TypeScript.

Deployment rules:

- Do not deploy without explicit confirmation.
- Prefer `bun alchemy deploy` after confirmation.
- Alchemy stores Cloudflare credentials in profiles via `alchemy login` or the first interactive deploy prompt.
- `alchemy.run.ts` creates an ignored `.alchemy/toki-agent-internal-token` file on first deploy/plan and binds it to the Worker as `TOKI_AGENT_INTERNAL_TOKEN`.
- Configure the Rust backend with `TOKI_AGENT__BASE_URL=<worker-url>` and `TOKI_AGENT__INTERNAL_TOKEN=<contents of .alchemy/toki-agent-internal-token>`.

Storage note:

The first implementation pass will use Durable Object storage for active run state. D1 remains the likely migration target once the run list and historical query requirements are clearer.

Cloudflare Sandbox note:

Alchemy v2 provisions the Worker, the run Durable Object namespace, the run Workflow, and the Cloudflare Sandbox container application. Cloudflare documents `wrangler.jsonc` declarations for Workflows and Sandbox containers, but the MVP wires the equivalent pieces through Alchemy:

- `TokiAgentWorker` exports `SandboxV2`, a thin subclass of the SDK `Sandbox` class, and binds `SANDBOX_V2`.
- `TokiAgentWorker` exports `AgentRunWorkflow`, creates the Cloudflare Workflow resource through `WorkflowResource`, and binds it as `AGENT_RUN_WORKFLOW`.
- The Worker uses the Sandbox SDK-required `nodejs_compat` compatibility flag.
- `SandboxContainerV3` builds from `docker.io/cloudflare/sandbox:0.9.2-opencode`. The Docker image version must stay synchronized with the `@cloudflare/sandbox` npm package version.
- The OpenCode base image supplies the Sandbox control server and OpenCode CLI. Alchemy appends its own container entrypoint after the Dockerfile, so the container resource must explicitly set `entrypoint: ["/container-server/sandbox"]` to preserve the Sandbox control-plane process.
- `SandboxContainerClass` adds Worker container metadata for `SandboxV2`.
- `SandboxDurableObject` attaches the container application to the `SANDBOX` Durable Object namespace.

The `Dockerfile` remains in the project because the Sandbox SDK still expects the Cloudflare Sandbox base image shape, but deployment is driven by `bun alchemy deploy`.

Cloudflare sign-in is not needed for local typecheck/tests. It is needed before the first `bun alchemy deploy` or if `alchemy login` is run explicitly.
