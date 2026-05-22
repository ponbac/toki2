import type { Env } from "./env";
import { routeRequest } from "./http/routes";
export { AgentRun } from "./runs/AgentRun";
export { SandboxV2 } from "./sandbox/Sandbox";
export { AgentRunWorkflow } from "./workflow/AgentRunWorkflow";

export default {
  fetch(request: Request, env: Env): Promise<Response> {
    return routeRequest(request, env);
  },
} satisfies ExportedHandler<Env>;
