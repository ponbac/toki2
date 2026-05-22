import { Result, Schema } from "effect";
import { describe, expect, it } from "vitest";
import { CreateAgentRunRequestSchema } from "../src/domain/schemas";

describe("create-run request schema", () => {
  it("accepts the MVP plan-first request shape", () => {
    const decoded = Schema.decodeUnknownResult(CreateAgentRunRequestSchema)({
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
        gitAuthHeader: "Basic redacted",
      },
      actor: {
        tokiUserId: 1,
        displayName: "Ada",
      },
    });

    expect(Result.isSuccess(decoded)).toBe(true);
  });

  it("rejects solve-immediately mode", () => {
    const decoded = Schema.decodeUnknownResult(CreateAgentRunRequestSchema)({
      mode: "solveImmediately",
      source: {},
      targetRepo: {},
      actor: {},
    });

    expect(Result.isFailure(decoded)).toBe(true);
  });
});
