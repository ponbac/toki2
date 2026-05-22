import { WorkflowEntrypoint, type WorkflowEvent, type WorkflowStep } from "cloudflare:workers";
import type { Env } from "../env";
import type { AgentRunRecord } from "../domain/schemas";
import type { AgentRunStatus } from "../domain/status";
import { isTerminalStatus } from "../domain/status";
import type { AgentRun } from "../runs/AgentRun";
import type { AgentRunPlanInput, AgentRunWorkflowParams } from "./agent-run-workflow-types";
import { PLAN_INPUT_EVENT_TYPE } from "./plan-input";

type RunSummary = {
  readonly runId: string;
  readonly status: AgentRunStatus;
  readonly planVersion: number;
};

export class AgentRunWorkflow extends WorkflowEntrypoint<Env, AgentRunWorkflowParams> {
  async run(
    event: WorkflowEvent<AgentRunWorkflowParams>,
    step: WorkflowStep,
  ): Promise<RunSummary | undefined> {
    const { runId } = event.payload;
    const stub = getRunStub(this.env, runId);
    let run = await step.do(
      "create-plan",
      { retries: { limit: 0, delay: "1 second" }, timeout: "30 minutes" },
      async () => summarizeRun(await stub.runPlanningStep()),
    );

    while (run !== undefined && !isTerminalStatus(run.status)) {
      if (run.status === "planApproved") {
        return await step.do(
          "implement-verify-publish",
          { retries: { limit: 0, delay: "1 second" }, timeout: "30 minutes" },
          async () => summarizeRun(await stub.runImplementationStep()),
        );
      }

      if (run.status === "awaitingPlanFeedback" || run.status === "revisingPlan") {
        await step.waitForEvent<AgentRunPlanInput>(`wait-for-plan-input-v${run.planVersion}`, {
          type: PLAN_INPUT_EVENT_TYPE,
          timeout: "365 days",
        });

        const latest = summarizeRun(await stub.getRun());

        if (latest?.status === "revisingPlan") {
          run = await step.do(
            `revise-plan-v${latest.planVersion + 1}`,
            { retries: { limit: 0, delay: "1 second" }, timeout: "30 minutes" },
            async () => summarizeRun(await stub.runRevisionStep()),
          );
          continue;
        }

        run = latest;
        continue;
      }

      return run;
    }

    return run;
  }
}

const getRunStub = (env: Env, runId: string): DurableObjectStub<AgentRun> => {
  if (env.AGENT_RUN === undefined) {
    throw new Error("AgentRun Durable Object binding is not configured.");
  }

  return env.AGENT_RUN.get(env.AGENT_RUN.idFromString(runId));
};

const summarizeRun = (run: AgentRunRecord | undefined): RunSummary | undefined =>
  run === undefined
    ? undefined
    : {
        runId: run.id,
        status: run.status,
        planVersion: run.workpad.planVersion,
      };
