import { describe, expect, it } from "vitest";
import { parseAgentWorkflow } from "../src/workflow/load-agent-workflow";
import { renderWorkflowPrompt } from "../src/workflow/render-prompts";
import type { CreateAgentRunRequest } from "../src/domain/schemas";

const run: CreateAgentRunRequest = {
  mode: "planFirst",
  source: {
    type: "adoWorkItem",
    id: "123",
    title: "Add launch action",
    url: "https://dev.azure.com/org/project/_workitems/edit/123",
    markdown: "# Work item\n\nBuild it.",
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
    tokiUserId: 7,
    displayName: "Ada",
  },
};

describe(".toki/agent.md parsing", () => {
  it("uses safe defaults when missing", () => {
    const workflow = parseAgentWorkflow(undefined);

    expect(workflow.source).toBe("default");
    expect(workflow.config.agent.harness).toBe("opencode");
    expect(workflow.config.verify.commands).toEqual(["just check", "just tsc", "just lint"]);
  });

  it("merges repo front matter with defaults", () => {
    const workflow = parseAgentWorkflow(`---
agent:
  harness: opencode
  max_turns: 3
sandbox:
  workspace_dir: /workspace/repo
setup:
  commands:
    - cargo fetch
verify:
  commands:
    - just check
publish:
  draft_pr: true
  branch_pattern: agent/{sourceType}-{sourceId}-{slug}
---
Issue context:
{{ issue.markdown }}
`);

    expect(workflow.source).toBe("repo");
    expect(workflow.config.agent.maxTurns).toBe(3);
    expect(workflow.config.setup.commands).toEqual(["cargo fetch"]);
    expect(workflow.config.verify.commands).toEqual(["just check"]);
  });
});

describe("prompt rendering", () => {
  it("renders supported issue and actor placeholders", () => {
    const workflow = parseAgentWorkflow("{{ issue.title }} for {{ actor.displayName }}\n{{ issue.markdown }}");

    expect(renderWorkflowPrompt(workflow, run)).toContain("Add launch action for Ada");
    expect(renderWorkflowPrompt(workflow, run)).toContain("Build it.");
  });
});
