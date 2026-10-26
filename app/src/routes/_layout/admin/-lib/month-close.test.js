import { describe, expect, test } from "bun:test";
import { formatInstant } from "./format.ts";
import { checkTargets, latestReadiness, problemText } from "./month-close.ts";

const machine = {
  userId: 3,
  fullName: "Ada Lovelace",
  machineId: "8d1f6a5e-2a4f-4a53-9a51-1d5b1f8f2c11",
  label: "ada-laptop",
};

describe("readiness problems in words", () => {
  test.each([
    [
      { kind: "residual", day: "2026-09-10", credits: "-5.00" },
      "Sep 10: −5 credits the accounts do not explain.",
    ],
    [
      { kind: "unmappedRepository", repository: "gadget-web", credits: "8.00" },
      "gadget-web: 8 credits of code review bill as Unassigned.",
    ],
    [
      {
        kind: "accountCheck",
        accountId: 1001,
        login: "ada-lovelace",
        outcome: { kind: "differs" },
      },
      "ada-lovelace: the stored days differ from GitHub's month total.",
    ],
    [
      {
        kind: "accountCheck",
        accountId: 1004,
        login: "retired-account",
        outcome: {
          kind: "interrupted",
          error: "the run stopped before finishing this month: interrupted",
        },
      },
      "retired-account: the run stopped before finishing this month: interrupted; the next run imports it again.",
    ],
    [
      { kind: "importRunning", startedAt: "2026-10-05T07:58:00Z" },
      `An import of the month is running (started ${formatInstant("2026-10-05T07:58:00Z")}): wait for it to finish.`,
    ],
    [
      {
        kind: "meteredPricing",
        accountId: 1001,
        login: "ada-lovelace",
        reasons: ["oldPricingBasis"],
      },
      "ada-lovelace: some tracked usage is on an old pricing basis.",
    ],
    [
      {
        kind: "oldBasis",
        ...machine,
        provider: "claude",
        buckets: 12,
        firstDay: "2026-09-03",
        lastDay: "2026-09-09",
        estimateUsd: "4.20",
      },
      "Ada Lovelace · ada-laptop · Claude Code: 12 buckets on an old pricing basis (Sep 3 – Sep 9), estimated at 4.20 USD.",
    ],
    [
      {
        kind: "rateProvisional",
        currency: "USD",
        observedThrough: "2026-09-29",
      },
      "The USD exchange rate is provisional: an average of the days through Sep 29.",
    ],
  ])("%#", (problem, text) => {
    expect(problemText(problem)).toBe(text);
  });
});

describe("where a failed check is resolved", () => {
  const check = (id, problems) => ({
    id,
    status: "failed",
    problems,
    waiver: null,
  });

  test("pricing opens uploads for machines and GitHub Copilot for accounts", () => {
    expect(
      checkTargets(
        check("pricing", [
          { kind: "meteredPricing", accountId: 1, login: "a", reasons: [] },
          { kind: "oldBasis", ...machine },
          { kind: "oldBasis", ...machine },
        ]),
      ),
    ).toEqual(["copilot", "data"]);
  });

  test("repositories open the mappings, but an unrecognized row the import", () => {
    expect(
      checkTargets(
        check("repositories", [
          { kind: "unmappedRepository", repository: "gadget-web" },
          { kind: "unrecognizedRepositoryRow" },
        ]),
      ),
    ).toEqual(["github-repositories", "copilot"]);
    expect(
      checkTargets(check("dispositions", [{ kind: "unlinkedCredits" }])),
    ).toEqual(["github-logins"]);
  });

  test("the later readiness wins", () => {
    const early = {
      evaluatedAt: "2026-10-05T08:00:00Z",
      ready: false,
      checks: [],
    };
    const late = { ...early, evaluatedAt: "2026-10-05T08:00:01Z" };

    expect(latestReadiness(early, late)).toBe(late);
    expect(latestReadiness(late, early)).toBe(late);
    expect(latestReadiness(early, null)).toBe(early);
  });
});
