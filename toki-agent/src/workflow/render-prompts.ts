import type { CreateAgentRunRequest } from "../domain/schemas";
import type { AgentWorkflow } from "./load-agent-workflow";

const replacementsForRun = (run: CreateAgentRunRequest): Readonly<Record<string, string>> => ({
  "issue.id": run.source.id,
  "issue.title": run.source.title,
  "issue.url": run.source.url,
  "issue.markdown": run.source.markdown,
  "actor.displayName": run.actor.displayName,
});

export function renderWorkflowPrompt(workflow: AgentWorkflow, run: CreateAgentRunRequest): string {
  const replacements = replacementsForRun(run);

  return workflow.promptPolicy.replace(/\{\{\s*([a-zA-Z0-9_.]+)\s*\}\}/g, (match, key: string) =>
    replacements[key] ?? match,
  );
}
