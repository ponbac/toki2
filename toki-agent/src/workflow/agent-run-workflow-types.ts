export type AgentRunWorkflowParams = {
  readonly runId: string;
};

export type AgentRunPlanInput =
  | {
      readonly action: "approve";
    }
  | {
      readonly action: "feedback";
      readonly message: string;
    };
