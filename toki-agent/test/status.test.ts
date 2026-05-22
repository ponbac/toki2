import { describe, expect, it } from "vitest";
import { canTransitionStatus, isTerminalStatus } from "../src/domain/status";

describe("agent run status transitions", () => {
  it("allows the MVP happy path", () => {
    expect(canTransitionStatus("created", "provisioningSandbox")).toBe(true);
    expect(canTransitionStatus("provisioningSandbox", "checkingRepositoryAccess")).toBe(true);
    expect(canTransitionStatus("checkingRepositoryAccess", "cloningRepository")).toBe(true);
    expect(canTransitionStatus("awaitingPlanFeedback", "planApproved")).toBe(true);
    expect(canTransitionStatus("creatingDraftPr", "draftPrCreated")).toBe(true);
    expect(canTransitionStatus("draftPrCreated", "succeeded")).toBe(true);
  });

  it("blocks transitions out of terminal statuses", () => {
    expect(isTerminalStatus("succeeded")).toBe(true);
    expect(canTransitionStatus("succeeded", "planning")).toBe(false);
    expect(canTransitionStatus("failed", "planning")).toBe(false);
    expect(canTransitionStatus("canceled", "planning")).toBe(false);
  });
});
