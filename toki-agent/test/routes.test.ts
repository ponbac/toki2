import { describe, expect, it } from "vitest";
import type { Env } from "../src/env";
import { routeRequest } from "../src/http/routes";
import type { AgentRun } from "../src/runs/AgentRun";
import type { AgentRunWorkflowParams } from "../src/workflow/agent-run-workflow-types";

const createRequest = {
  mode: "planFirst",
  source: {
    type: "adoWorkItem",
    id: "123",
    title: "Agent MVP",
    url: "https://example.invalid/work-item/123",
    markdown: "Markdown context",
  },
  targetRepo: {
    provider: "azureDevOps",
    cloneUrl: "https://dev.azure.com/org/project/_git/repo",
    defaultBranch: "main",
    organization: "org",
    project: "project",
    repoName: "repo",
  },
  actor: {
    tokiUserId: 1,
    displayName: "Ada",
  },
} as const;

const makeAuthorizedRequest = (url: string, init?: RequestInit) =>
  new Request(url, {
    ...init,
    headers: {
      authorization: "Bearer test-token",
      "content-type": "application/json",
      ...init?.headers,
    },
  });

const makeEnv = (): Env & {
  readonly workflowCreates: Array<WorkflowInstanceCreateOptions<AgentRunWorkflowParams>>;
  readonly workflowEvents: Array<{ readonly type: string; readonly payload: unknown }>;
} => {
  const runs = new Map<string, Awaited<ReturnType<AgentRun["createRun"]>>>();
  const workflowCreates: Array<WorkflowInstanceCreateOptions<AgentRunWorkflowParams>> = [];
  const workflowEvents: Array<{ readonly type: string; readonly payload: unknown }> = [];
  const id = {
    toString: () => "run-1",
  };

  const makeStub = (runId: string) => ({
    createRun: async (_id: string, request: typeof createRequest) => {
      const now = "2026-05-01T00:00:00.000Z";
      const record: Awaited<ReturnType<AgentRun["createRun"]>> = {
        id: runId,
        status: "created",
        mode: "planFirst",
        source: request.source,
        targetRepo: request.targetRepo,
        actor: request.actor,
        workpad: {
          currentPlanMarkdown: "",
          planVersion: 0,
          feedbackHistory: [],
          acceptanceCriteria: [],
          validationChecklist: [],
          notes: [],
          risksAndConfusions: [],
        },
        events: [
          {
            id: "event-1",
            runId,
            status: "created",
            message: "Run created.",
            createdAt: now,
          },
        ],
        createdAt: now,
        updatedAt: now,
      };

      runs.set(runId, record);
      return record;
    },
    getRun: async () => runs.get(runId),
    getEvents: async () => runs.get(runId)?.events ?? [],
    addFeedback: async () => runs.get(runId),
    approvePlan: async () => {
      const run = runs.get(runId);

      if (run === undefined) {
        return undefined;
      }

      const nextRun = {
        ...run,
        status: "planApproved" as const,
      };
      runs.set(runId, nextRun);
      return nextRun;
    },
    cancel: async () => runs.get(runId),
    deleteRun: async () => runs.delete(runId),
  });

  return {
    TOKI_AGENT_INTERNAL_TOKEN: "test-token",
    workflowCreates,
    workflowEvents,
    AGENT_RUN: {
      newUniqueId: () => id,
      idFromString: (runId: string) => ({
        toString: () => runId,
      }),
      get: (runId: { toString: () => string }) => makeStub(runId.toString()),
    } as unknown as DurableObjectNamespace<AgentRun>,
    AGENT_RUN_WORKFLOW: {
      create: async (options?: WorkflowInstanceCreateOptions<AgentRunWorkflowParams>) => {
        if (options !== undefined) {
          workflowCreates.push(options);
        }
        return { id: options?.id ?? "workflow-1" };
      },
      get: async (runId: string) => ({
        id: runId,
        sendEvent: async (event: { readonly type: string; readonly payload: unknown }) => {
          workflowEvents.push(event);
        },
      }),
    } as unknown as Workflow<AgentRunWorkflowParams>,
  };
};

describe("worker routes", () => {
  it("serves health JSON", async () => {
    const response = await routeRequest(new Request("https://agent.example/health"), {});

    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({
      ok: true,
      service: "toki-agent",
    });
  });

  it("requires internal auth for internal routes", async () => {
    const response = await routeRequest(new Request("https://agent.example/internal/runs"), {});

    expect(response.status).toBe(500);
    await expect(response.json()).resolves.toMatchObject({
      error: "InternalAuthNotConfigured",
    });
  });

  it("creates and reads an agent run through the Durable Object binding", async () => {
    const env = makeEnv();

    const createResponse = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs", {
        method: "POST",
        body: JSON.stringify(createRequest),
      }),
      env,
    );

    expect(createResponse.status).toBe(201);
    await expect(createResponse.json()).resolves.toMatchObject({
      id: "run-1",
      status: "created",
    });
    expect(env.workflowCreates).toEqual([
      {
        id: "run-1",
        params: { runId: "run-1" },
      },
    ]);

    const getResponse = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/run-1"),
      env,
    );

    expect(getResponse.status).toBe(200);
    await expect(getResponse.json()).resolves.toMatchObject({
      id: "run-1",
      source: {
        id: "123",
      },
    });
  });

  it("returns run events", async () => {
    const env = makeEnv();

    await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs", {
        method: "POST",
        body: JSON.stringify(createRequest),
      }),
      env,
    );

    const response = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/run-1/events"),
      env,
    );

    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual([
      expect.objectContaining({
        runId: "run-1",
        status: "created",
      }),
    ]);
  });

  it("deletes an agent run", async () => {
    const env = makeEnv();

    await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs", {
        method: "POST",
        body: JSON.stringify(createRequest),
      }),
      env,
    );

    const deleteResponse = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/run-1", {
        method: "DELETE",
      }),
      env,
    );

    expect(deleteResponse.status).toBe(200);
    await expect(deleteResponse.json()).resolves.toEqual({ deleted: true });

    const getResponse = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/run-1"),
      env,
    );

    expect(getResponse.status).toBe(404);
  });

  it("sends plan input events to the run workflow", async () => {
    const env = makeEnv();

    await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs", {
        method: "POST",
        body: JSON.stringify(createRequest),
      }),
      env,
    );

    const feedbackResponse = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/run-1/feedback", {
        method: "POST",
        body: JSON.stringify({
          message: "Tighten the plan.",
          actor: createRequest.actor,
        }),
      }),
      env,
    );

    expect(feedbackResponse.status).toBe(200);

    const approveResponse = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/run-1/approve-plan", {
        method: "POST",
      }),
      env,
    );

    expect(approveResponse.status).toBe(200);
    expect(env.workflowEvents).toEqual([
      {
        type: "plan-input",
        payload: {
          action: "feedback",
          message: "Tighten the plan.",
        },
      },
      {
        type: "plan-input",
        payload: {
          action: "approve",
        },
      },
    ]);
  });

  it("returns JSON 404 for invalid run IDs", async () => {
    const env = makeEnv();
    const namespace = env.AGENT_RUN;

    if (namespace === undefined) {
      throw new Error("test env should include an AgentRun namespace");
    }

    const invalidIdEnv: Env = {
      ...env,
      AGENT_RUN: {
        ...namespace,
        idFromString: () => {
          throw new Error("invalid object ID");
        },
      } as unknown as DurableObjectNamespace<AgentRun>,
    };

    const response = await routeRequest(
      makeAuthorizedRequest("https://agent.example/internal/runs/not-real"),
      invalidIdEnv,
    );

    expect(response.status).toBe(404);
    await expect(response.json()).resolves.toMatchObject({
      error: "RunNotFound",
    });
  });

  it("returns JSON 404 for unknown routes", async () => {
    const response = await routeRequest(new Request("https://agent.example/not-found"), {});

    expect(response.status).toBe(404);
    await expect(response.json()).resolves.toMatchObject({
      error: "NotFound",
    });
  });
});
