import { describe, expect, it } from "vitest";
import type { CreateAgentRunRequest } from "../src/domain/schemas";
import {
  buildAzureDevOpsPullRequestBody,
  buildAzureDevOpsPullRequestUrl,
  buildAzureDevOpsPullRequestWebUrl,
  buildDraftPrDescription,
  generateBranchName,
  gitUrlWithUsername,
} from "../src/publish/publish-service";

const run: CreateAgentRunRequest = {
  mode: "planFirst",
  source: {
    type: "adoWorkItem",
    id: "123",
    title: "Launch agent from board!",
    url: "https://dev.azure.com/org/project/_workitems/edit/123",
    markdown: "Work item markdown",
  },
  targetRepo: {
    provider: "azureDevOps",
    cloneUrl: "https://dev.azure.com/org/project/_git/repo",
    defaultBranch: "main",
    organization: "org",
    project: "project name",
    repoName: "repo",
  },
  actor: {
    tokiUserId: 1,
    displayName: "Ada",
  },
};

describe("publishing helpers", () => {
  it("generates branch names from the configured pattern", () => {
    expect(generateBranchName(run, "agent/{sourceType}-{sourceId}-{slug}")).toBe(
      "agent/adoWorkItem-123-launch-agent-from-board",
    );
  });

  it("builds a concise draft PR description focused on summary, important files, why, and validation", () => {
    const description = buildDraftPrDescription({
      run,
      implementationSummary: [
        "I’ll inspect the existing board flow before making changes.",
        "",
        "Summary:",
        "- Added the launch-agent button to board cards.",
        "- Wired the launch dialog to agent run creation.",
        "",
        "Per instruction, I did not run validation, lint, build, or tests.",
      ].join("\n"),
      validation: [{ command: "just check", exitCode: 0 }],
      changedFiles: ["app/src/routes/_layout/board/-components/board-card.tsx"],
    });

    expect(description).toContain("Launch agent from board!");
    expect(description).toContain("- Added the launch-agent button to board cards.");
    expect(description).toContain("- Wired the launch dialog to agent run creation.");
    expect(description).not.toContain("I’ll inspect");
    expect(description).not.toContain("Per instruction");
    expect(description).toContain("- `app/src/routes/_layout/board/-components/board-card.tsx`");
    expect(description).toContain("## Why");
    expect(description).toContain(
      "- [Launch agent from board!](https://dev.azure.com/org/project/_workitems/edit/123)",
    );
    expect(description).toContain("`just check`: passed");
    expect(description).not.toContain("Accepted Plan");
  });

  it("builds Azure DevOps draft PR request details", () => {
    expect(buildAzureDevOpsPullRequestUrl(run.targetRepo)).toBe(
      "https://dev.azure.com/org/project%20name/_apis/git/repositories/repo/pullrequests?api-version=7.1",
    );
    expect(buildAzureDevOpsPullRequestWebUrl(run.targetRepo, 42)).toBe(
      "https://dev.azure.com/org/project%20name/_git/repo/pullrequest/42",
    );
    expect(
      buildAzureDevOpsPullRequestBody({
        targetRepo: run.targetRepo,
        gitAuthHeader: "Basic token",
        sourceBranch: "agent/adoWorkItem-123-launch-agent-from-board",
        title: "Agent: Launch agent from board!",
        description: "body",
      }),
    ).toEqual({
      sourceRefName: "refs/heads/agent/adoWorkItem-123-launch-agent-from-board",
      targetRefName: "refs/heads/main",
      title: "Agent: Launch agent from board!",
      description: "body",
      isDraft: true,
    });
  });

  it("adds a non-secret username to HTTPS git remotes for askpass pushes", () => {
    expect(
      gitUrlWithUsername(
        "https://dev.azure.com/org/project%20name/_git/repo",
        "toki-agent",
      ),
    ).toBe("https://toki-agent@dev.azure.com/org/project%20name/_git/repo");
    expect(gitUrlWithUsername("git@ssh.dev.azure.com:v3/org/project/repo", "toki-agent")).toBeUndefined();
  });
});
