# Toki Agent MVP Handoff

Date: 2026-05-03

## Current Goal

Finish and prove the first MVP of a board-managed coding agent:

- Launch an agent from the Toki board.
- Run plan-first.
- Accept plan feedback and regenerate the plan.
- After approval, clone the target repo in Cloudflare Sandbox.
- Use real git and real OpenCode CLI inside the Sandbox.
- Validate with the repo workflow.
- Commit, push a branch, and create a draft PR in the target repo.

The current target validation case is Lerum issue `#3838` in Azure DevOps repo `lerumsdjur / Lerums Djursjukhus / LD.Apport`.

## Current Status

The vertical slice is implemented and mostly working.

Working:

- `toki-agent/` Cloudflare Worker, Durable Object, Workflow, and Sandbox container are deployed through Alchemy v2.
- Internal Worker API exists:
  - `POST /internal/runs`
  - `GET /internal/runs/:id`
  - `GET /internal/runs/:id/events`
  - `POST /internal/runs/:id/feedback`
  - `POST /internal/runs/:id/approve-plan`
  - `POST /internal/runs/:id/cancel`
- Rust backend proxy routes exist under `/agent-runs`.
- Frontend board UI has the launch dialog and agent drawer.
- Runs can plan, accept feedback, revise plans, receive approval, implement in Sandbox, validate, commit, and attempt publish.
- Cloudflare Sandbox is available. The old Azure DevOps REST-only planning fallback is not the current path.
- OpenCode runs correctly with the OpenAI provider when configured as `openai/gpt-5.5 --variant low`.
- Validation workflow discovery has been tightened for Lerum:
  - frontend uses `pnpm check` when available.
  - backend uses `dotnet build --no-restore`.
  - Lerum E2E tests must not run.
  - `dotnet test` must not run.

Not yet proven after the latest fixes:

- A full Lerum `#3838` run has not yet produced the final draft PR.
- The Azure DevOps push authentication bug was fixed in code, but has not yet been revalidated by a successful pushed branch and draft PR.
- OpenCode progress is now streamed from Sandbox logs, but the updated UX still needs to be validated in the browser.

## Deployment And Auth

Use OpenCode's OpenAI provider, not the OpenCode provider namespace:

```bash
cd toki-agent
env -u OPENAI_API_KEY OPENCODE_MODEL=openai/gpt-5.5 OPENCODE_VARIANT=low bun --bun alchemy deploy --yes
```

Important auth details:

- Do not copy or use Codex config.
- Do not pass the Spindexer `OPENAI_API_KEY` for this flow.
- `alchemy.run.ts` reads local OpenCode auth from `~/.local/share/opencode/auth.json` when `OPENAI_API_KEY` is unset and deploys it as `OPENCODE_AUTH_JSON`.
- The Sandbox container uses the standalone OpenCode binary at `/root/.opencode/bin/opencode`.
- Correct model command shape was verified locally:

```bash
opencode run -m openai/gpt-5.5 --variant low 'Respond exactly OK.'
```

This returned `OK`.

Known bad configurations:

- `opencode/gpt-5.5` fails with `Model not found`.
- Explicit Spindexer `OPENAI_API_KEY` failed in Sandbox with quota errors.

Do not print PATs, internal tokens, OpenCode auth JSON, or OpenAI credentials.

## Lerum Test History

Local DB was pulled from prod and contains the Lerum PAT for `LD.Apport`.

Key run outcomes:

| Run | Outcome |
| --- | --- |
| `583c088a...` | Failed because it used wrong model namespace `opencode/gpt-5.5`. |
| `44970f64...` | Failed with quota error after deploying explicit Spindexer OpenAI API key. |
| `aaea2904...` | Planned, revised, implemented, and validated, but repair modified package lifecycle scripts. The run was canceled before PR because that violated the intended validation constraints. |
| `3edccd8...` | Implemented and validated correctly. First `pnpm check` failed on missing generated `build-info`; repair added a minimal generated module. Frontend `pnpm check` and backend `dotnet build --no-restore` passed. Publish failed while pushing to Azure DevOps because git could not read credentials for `https://dev.azure.com`. |
| `64d6db6d...` | Started after the push-auth fix. Planning and feedback worked. Implementation then stayed silent for roughly 15 minutes and was canceled. This exposed the need for better OpenCode progress visibility. |

No Lerum draft PR has been created yet.

## Latest Code Changes

Main files changed for the current state:

- `toki-agent/src/runs/AgentRun.ts`
  - Runs the full plan/revise/approve/implement/validate/publish state machine.
  - Passes model, variant, OpenAI/Gemini env, and OpenCode auth into Sandbox execution.
  - Uses Lerum-safe validation discovery: frontend check and backend build only.
  - Installs `pnpm` globally when needed.
  - Publishes through git with Azure DevOps PAT support.
  - Adds progress notes during planning, revision, implementation, and repair.
- `toki-agent/src/sandbox/sandbox-service.ts`
  - `execLong` now polls Sandbox process logs while a command is running.
  - Emits stdout/stderr deltas every few seconds so long OpenCode runs are visible.
- `toki-agent/src/opencode/opencode-service.ts`
  - Invokes `/root/.opencode/bin/opencode`.
  - Adds `--variant`.
  - Extracts sanitized, deduplicated progress lines from OpenCode output.
  - Redacts secrets and strips ANSI escape sequences.
- `toki-agent/src/publish/publish-service.ts`
  - Push now uses a temporary `GIT_ASKPASS` helper plus `http.extraHeader`.
  - Forces `http.version=HTTP/1.1`.
  - Removes the helper after push.
- `toki-agent/alchemy.run.ts`
  - Wires `OPENCODE_MODEL`, `OPENCODE_VARIANT`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, and `OPENCODE_AUTH_JSON`.
  - Reads local OpenCode auth JSON when no OpenAI key is provided.
- `toki-agent/Dockerfile`
  - Based on `cloudflare/sandbox:0.9.2-opencode`.
  - Installs build dependencies, Rust, .NET, and the current standalone OpenCode binary.

## Validation State

Latest completed check after the OpenCode progress-streaming patch:

```bash
cd toki-agent && bun run check
```

This passed.

Earlier in this branch, these also passed, but they were not rerun after the latest progress-streaming changes:

```bash
just tsc
just lint
SQLX_OFFLINE=true just check
just --fmt --check
```

The worktree is intentionally large because this is the full MVP slice. Do not revert unrelated files.

## Timings Observed

Typical timings from the Lerum runs:

- Sandbox provisioning, clone, and workflow load: about 8-10 seconds.
- OpenCode planning: about 40-55 seconds.
- Plan revision: about 10-15 seconds.
- Successful implementation in earlier runs: about 35-70 seconds.
- `pnpm install`: about 20-30 seconds.
- .NET restore/setup: about 60-90 seconds.
- Frontend `pnpm check`: about 20-40 seconds.
- Backend `dotnet build --no-restore`: passed after frontend repair in the successful validation run.

The problematic latest run was silent during implementation from roughly `07:57:56` to cancellation at `08:13:29`. The new log polling and progress notes were added specifically to diagnose this.

## How To Continue

1. Deploy the current agent:

```bash
cd toki-agent
env -u OPENAI_API_KEY OPENCODE_MODEL=openai/gpt-5.5 OPENCODE_VARIANT=low bun --bun alchemy deploy --yes
```

2. Start the local Toki app if needed:

```bash
just dev
```

3. Use the board and Playwriter to run Lerum issue `#3838`:

```text
http://localhost:5173/board?organization=lerumsdjur&project=Lerums%20Djursjukhus&iterationPath=Lerums%20Djursjukhus%5CSprint%2041
```

4. Confirm the generated plan does not add E2E tests, `dotnet test`, or package lifecycle-script changes.

5. Approve the plan and watch the drawer notes for lines like:

```text
OpenCode progress: ...
```

6. If validation passes, the publisher should now push with the Azure DevOps PAT and create a draft PR in `LD.Apport`.

Final acceptance for this slice:

- Lerum issue `#3838` run reaches `published`.
- Draft PR exists in `LD.Apport`, not in this Toki repo.
- UI shows useful current-agent activity during long OpenCode stages.
- Report stage timings from the run.

## Known Risks

- Push auth is fixed in code but not proven by a successful post-fix draft PR.
- OpenCode may still hang or emit no useful output. If that happens, the new progress polling should make the failure mode visible.
- The agent may drift into changing validation scripts or running tests. For Lerum, reject or revise plans that include E2E, `dotnet test`, or lifecycle script edits.
- The generated target-repo changes from earlier runs were only inside Cloudflare Sandbox target clones; they are not committed in this Toki repo.

## Key Files

Agent Worker:

- `toki-agent/src/runs/AgentRun.ts`
- `toki-agent/src/sandbox/sandbox-service.ts`
- `toki-agent/src/opencode/opencode-service.ts`
- `toki-agent/src/publish/publish-service.ts`
- `toki-agent/src/workflow/load-agent-workflow.ts`
- `toki-agent/alchemy.run.ts`
- `toki-agent/Dockerfile`

Backend:

- `toki-api/src/routes/agent_runs.rs`
- `toki-api/src/app_state.rs`
- `toki-api/src/config.rs`
- `toki-api/config/base.yaml`
- `az-devops/src/repo_client.rs`

Frontend:

- `app/src/routes/_layout/board/-components/board-view.tsx`
- `app/src/routes/_layout/board/-components/board-card.tsx`
- `app/src/routes/_layout/board/-components/launch-agent-dialog.tsx`
- `app/src/routes/_layout/board/-components/agent-run-drawer.tsx`
- `app/src/lib/api/queries/agentRuns.ts`
- `app/src/lib/api/mutations/agentRuns.ts`

Playwriter skill:

- `/home/ponbac/.agents/skills/playwriter/SKILL.md`
