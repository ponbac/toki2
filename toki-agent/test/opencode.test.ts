import { describe, expect, it } from "vitest";
import type { AgentRunRecord } from "../src/domain/schemas";
import {
  buildPlanPrompt,
  buildVerificationRepairPrompt,
  extractPlanMarkdown,
} from "../src/opencode/opencode-service";
import { DEFAULT_AGENT_WORKFLOW } from "../src/workflow/load-agent-workflow";

const run: AgentRunRecord = {
  id: "run-1",
  status: "planning",
  mode: "planFirst",
  source: {
    type: "adoWorkItem",
    id: "123",
    title: "Agent MVP",
    url: "https://example.invalid/123",
    markdown: "Work item markdown",
  },
  targetRepo: {
    provider: "azureDevOps",
    cloneUrl: "https://dev.azure.com/org/project/_git/repo",
    defaultBranch: "main",
  },
  actor: {
    tokiUserId: 1,
    displayName: "Ada",
  },
  workpad: {
    currentPlanMarkdown: "",
    planVersion: 0,
    feedbackHistory: [],
    acceptanceCriteria: [],
    validationChecklist: [],
    notes: [],
    risksAndConfusions: [],
  },
  events: [],
  createdAt: "2026-05-01T00:00:00.000Z",
  updatedAt: "2026-05-01T00:00:00.000Z",
};

describe("opencode planning helpers", () => {
  it("renders the workflow prompt with run context", () => {
    const prompt = buildPlanPrompt(DEFAULT_AGENT_WORKFLOW, run);

    expect(prompt).toContain("Work item markdown");
    expect(prompt).toContain("First produce an implementation plan");
  });

  it("removes OpenCode progress preamble from generated plans", () => {
    expect(
      extractPlanMarkdown(
        [
          "I’m going to inspect the code first.",
          "I found the likely route.",
          "**Plan**",
          "1. Inspect the endpoint.",
          "2. Add focused tests.",
        ].join("\n"),
      ),
    ).toBe("**Plan**\n1. Inspect the endpoint.\n2. Add focused tests.");
  });

  it("builds a bounded repair prompt from verification output", () => {
    const prompt = buildVerificationRepairPrompt({
      run: {
        ...run,
        workpad: {
          ...run.workpad,
          currentPlanMarkdown: "**Plan**\nFix the query.",
        },
      },
      command: "dotnet build",
      stdout: "Compilation failed.",
      stderr: "CS0103 missing symbol",
      attempt: 1,
      maxAttempts: 2,
    });

    expect(prompt).toContain("Repair attempt: 1/2");
    expect(prompt).toContain("dotnet build");
    expect(prompt).toContain("CS0103 missing symbol");
    expect(prompt).toContain("Do not run project validation");
  });
});
