// biome-ignore-all lint: Worker HTTP routing is a request boundary; route guard clauses keep method/path dispatch explicit.
import type { Env } from "../env";
import { Match, Result, Schema } from "effect";
import {
  CompletePublishRequestSchema,
  CreateAgentRunRequestSchema,
  FailPublishRequestSchema,
  FeedbackRequestSchema,
} from "../domain/schemas";
import type { AgentRun } from "../runs/AgentRun";
import type { AgentRunWorkflowParams } from "../workflow/agent-run-workflow-types";
import { sendPlanInput } from "../workflow/plan-input";

const jsonResponse = (body: unknown, init?: ResponseInit): Response =>
  new Response(JSON.stringify(body), {
    ...init,
    headers: {
      "content-type": "application/json; charset=utf-8",
      ...init?.headers,
    },
  });

const jsonError = (status: number, error: string, message: string): Response =>
  jsonResponse(
    {
      error,
      message,
    },
    { status },
  );

const requireInternalAuth = (request: Request, env: Env): Response | undefined => {
  const configuredToken = env.TOKI_AGENT_INTERNAL_TOKEN;

  return Match.value(configuredToken).pipe(
    Match.when(
      (token): token is string => typeof token === "string" && token.length > 0,
      (token) => authorizeRequest(request, token),
    ),
    Match.orElse(() =>
      jsonError(500, "InternalAuthNotConfigured", "TOKI_AGENT_INTERNAL_TOKEN is not configured."),
    ),
  );
};

const authorizeRequest = (request: Request, token: string): Response | undefined =>
  Match.value(request.headers.get("authorization") === `Bearer ${token}`).pipe(
    Match.when(true, () => undefined),
    Match.orElse(() =>
      jsonError(401, "Unauthorized", "Missing or invalid internal authorization token."),
    ),
  );

const readJson = async (request: Request): Promise<unknown> => await request.json().catch(() => undefined);

const getRunNamespace = (env: Env): DurableObjectNamespace<AgentRun> | Response =>
  Match.value(env.AGENT_RUN).pipe(
    Match.when(Match.undefined, () =>
      jsonError(500, "RunStorageNotConfigured", "AgentRun Durable Object binding is not configured."),
    ),
    Match.orElse((namespace) => namespace),
  );

const getRunWorkflow = (env: Env): Workflow<AgentRunWorkflowParams> | Response =>
  Match.value(env.AGENT_RUN_WORKFLOW).pipe(
    Match.when(Match.undefined, () =>
      jsonError(500, "RunWorkflowNotConfigured", "AgentRun Workflow binding is not configured."),
    ),
    Match.orElse((workflow) => workflow),
  );

const getRunStub = (env: Env, id: string) => {
  return Match.value(env.AGENT_RUN).pipe(
    Match.when(Match.undefined, () =>
      jsonError(500, "RunStorageNotConfigured", "AgentRun Durable Object binding is not configured."),
    ),
    Match.orElse((namespace) => {
      try {
        return namespace.get(namespace.idFromString(id));
      } catch {
        return jsonError(404, "RunNotFound", "Agent run was not found.");
      }
    }),
  );
};

const jsonRunOrNotFound = (run: unknown | undefined): Response =>
  Match.value(run).pipe(
    Match.when(Match.undefined, () => jsonError(404, "RunNotFound", "Agent run was not found.")),
    Match.orElse((record) => jsonResponse(record)),
  );

const getRunIdFromPath = (pathname: string, suffix?: string): string | undefined => {
  const prefix = "/internal/runs/";

  return Match.value(pathname.startsWith(prefix)).pipe(
    Match.when(false, () => undefined),
    Match.orElse(() => getRunIdFromTail(pathname.slice(prefix.length), suffix)),
  );
};

const getRunIdFromTail = (tail: string, suffix?: string): string | undefined =>
  Match.value(suffix).pipe(
    Match.when(Match.undefined, () => getUnsuffixedRunId(tail)),
    Match.orElse((definedSuffix) => getSuffixedRunId(tail, definedSuffix)),
  );

const getUnsuffixedRunId = (tail: string): string | undefined =>
  Match.value(tail.includes("/")).pipe(
    Match.when(true, () => undefined),
    Match.orElse(() => tail),
  );

const getSuffixedRunId = (tail: string, suffix: string): string | undefined => {
  const suffixWithSlash = `/${suffix}`;

  return Match.value(tail.endsWith(suffixWithSlash)).pipe(
    Match.when(true, () => tail.slice(0, -suffixWithSlash.length)),
    Match.orElse(() => undefined),
  );
};

export async function routeRequest(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);

  if (request.method === "GET" && url.pathname === "/health") {
    return jsonResponse({ ok: true, service: "toki-agent" });
  }

  if (url.pathname.startsWith("/internal/")) {
    const authError = requireInternalAuth(request, env);

    if (authError !== undefined) {
      return authError;
    }
  }

  if (request.method === "POST" && url.pathname === "/internal/runs") {
    const namespace = getRunNamespace(env);

    if (namespace instanceof Response) {
      return namespace;
    }

    const decoded = Schema.decodeUnknownResult(CreateAgentRunRequestSchema)(await readJson(request));

    if (Result.isFailure(decoded)) {
      return jsonError(400, "InvalidCreateRunRequest", "Create-run request does not match the internal API schema.");
    }

    const workflow = getRunWorkflow(env);

    if (workflow instanceof Response) {
      return workflow;
    }

    const id = namespace.newUniqueId();
    const runId = id.toString();
    const stub = namespace.get(id);
    let run = await stub.createRun(runId, decoded.success);

    try {
      await workflow.create({
        id: runId,
        params: { runId },
      });
    } catch (error) {
      run =
        (await stub.recordWorkflowStartFailure(
          error instanceof Error ? error.message : "Failed to start agent workflow.",
        )) ?? run;
      return jsonResponse(run, { status: 500 });
    }

    return jsonResponse(run, { status: 201 });
  }

  const eventsRunId = getRunIdFromPath(url.pathname, "events");

  if (request.method === "GET" && eventsRunId !== undefined) {
    const stub = getRunStub(env, eventsRunId);

    if (stub instanceof Response) {
      return stub;
    }

    return jsonResponse(await stub.getEvents());
  }

  const feedbackRunId = getRunIdFromPath(url.pathname, "feedback");

  if (request.method === "POST" && feedbackRunId !== undefined) {
    const stub = getRunStub(env, feedbackRunId);

    if (stub instanceof Response) {
      return stub;
    }

    const workflow = getRunWorkflow(env);

    if (workflow instanceof Response) {
      return workflow;
    }

    const decoded = Schema.decodeUnknownResult(FeedbackRequestSchema)(await readJson(request));

    if (Result.isFailure(decoded)) {
      return jsonError(400, "InvalidFeedbackRequest", "Feedback request does not match the internal API schema.");
    }

    const run = await stub.addFeedback(decoded.success, decoded.success.actor);

    if (run !== undefined) {
      try {
        await sendPlanInput(workflow, feedbackRunId, {
          action: "feedback",
          message: decoded.success.message,
        });
      } catch (error) {
        return jsonError(
          500,
          "RunWorkflowNotificationFailed",
          error instanceof Error ? error.message : "Failed to notify agent workflow.",
        );
      }
    }

    return jsonRunOrNotFound(run);
  }

  const approveRunId = getRunIdFromPath(url.pathname, "approve-plan");

  if (request.method === "POST" && approveRunId !== undefined) {
    const stub = getRunStub(env, approveRunId);

    if (stub instanceof Response) {
      return stub;
    }

    const workflow = getRunWorkflow(env);

    if (workflow instanceof Response) {
      return workflow;
    }

    const run = await stub.approvePlan();

    if (run?.status === "planApproved") {
      try {
        await sendPlanInput(workflow, approveRunId, {
          action: "approve",
        });
      } catch (error) {
        return jsonError(
          500,
          "RunWorkflowNotificationFailed",
          error instanceof Error ? error.message : "Failed to notify agent workflow.",
        );
      }
    }

    return jsonRunOrNotFound(run);
  }

  const cancelRunId = getRunIdFromPath(url.pathname, "cancel");

  if (request.method === "POST" && cancelRunId !== undefined) {
    const stub = getRunStub(env, cancelRunId);

    if (stub instanceof Response) {
      return stub;
    }

    const run = await stub.cancel();

    return jsonRunOrNotFound(run);
  }

  const claimPublishRunId = getRunIdFromPath(url.pathname, "claim-publish");

  if (request.method === "POST" && claimPublishRunId !== undefined) {
    const stub = getRunStub(env, claimPublishRunId);

    if (stub instanceof Response) {
      return stub;
    }

    const run = await stub.claimBackendPublish();

    return jsonRunOrNotFound(run);
  }

  const completePublishRunId = getRunIdFromPath(url.pathname, "complete-publish");

  if (request.method === "POST" && completePublishRunId !== undefined) {
    const stub = getRunStub(env, completePublishRunId);

    if (stub instanceof Response) {
      return stub;
    }

    const decoded = Schema.decodeUnknownResult(CompletePublishRequestSchema)(await readJson(request));

    if (Result.isFailure(decoded)) {
      return jsonError(400, "InvalidCompletePublishRequest", "Complete-publish request does not match the internal API schema.");
    }

    const run = await stub.completeBackendPublish(decoded.success);

    return jsonRunOrNotFound(run);
  }

  const failPublishRunId = getRunIdFromPath(url.pathname, "fail-publish");

  if (request.method === "POST" && failPublishRunId !== undefined) {
    const stub = getRunStub(env, failPublishRunId);

    if (stub instanceof Response) {
      return stub;
    }

    const decoded = Schema.decodeUnknownResult(FailPublishRequestSchema)(await readJson(request));

    if (Result.isFailure(decoded)) {
      return jsonError(400, "InvalidFailPublishRequest", "Fail-publish request does not match the internal API schema.");
    }

    const run = await stub.failBackendPublish(decoded.success);

    return jsonRunOrNotFound(run);
  }

  const runId = getRunIdFromPath(url.pathname);

  if (request.method === "DELETE" && runId !== undefined) {
    const stub = getRunStub(env, runId);

    if (stub instanceof Response) {
      return stub;
    }

    const deleted = await stub.deleteRun();

    if (!deleted) {
      return jsonError(404, "RunNotFound", "Agent run was not found.");
    }

    return jsonResponse({ deleted: true });
  }

  if (request.method === "GET" && runId !== undefined) {
    const stub = getRunStub(env, runId);

    if (stub instanceof Response) {
      return stub;
    }

    const run = await stub.getRun();

    return jsonRunOrNotFound(run);
  }

  return jsonResponse(
    {
      error: "NotFound",
      message: `No route for ${request.method} ${url.pathname}`,
    },
    { status: 404 },
  );
}
