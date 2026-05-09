import type { AgentRunPlanInput, AgentRunWorkflowParams } from "./agent-run-workflow-types";

export const PLAN_INPUT_EVENT_TYPE = "plan-input";

export const sendPlanInput = async (
  workflow: Workflow<AgentRunWorkflowParams>,
  runId: string,
  payload: AgentRunPlanInput,
): Promise<void> => {
  const instance = await workflow.get(runId);
  await instance.sendEvent({
    type: PLAN_INPUT_EVENT_TYPE,
    payload,
  });
};
