import { describe, expect, test } from "bun:test";
import {
  CLOSE_CHECKS,
  checkWaivable,
  importRunningSince,
  parseAiCloseConflict,
  parseAiMonthCloseStatus,
  waiverReasonProblem,
} from "./ai-month-close.ts";

const machine = {
  userId: 3,
  fullName: "Ada Lovelace",
  machineId: "8d1f6a5e-2a4f-4a53-9a51-1d5b1f8f2c11",
  label: "ada-laptop",
};

/** September's checks as the server evaluates them, one problem of each
 * kind a check can find. */
const checks = [
  {
    id: "import",
    status: "failed",
    problems: [
      { kind: "importRunning", startedAt: "2026-10-05T07:58:00Z" },
      {
        kind: "notSettled",
        lastDay: "2026-09-30",
        settlesOn: "2026-10-04",
        settleDays: 3,
      },
      {
        kind: "daysBeforeBackfill",
        firstDay: "2026-09-01",
        lastDay: "2026-09-02",
      },
      {
        kind: "daysNotImported",
        source: "orgUsage",
        days: 2,
        firstDay: "2026-09-10",
        lastDay: "2026-09-11",
      },
      {
        kind: "accountCheck",
        accountId: 1001,
        login: "ada-lovelace",
        outcome: { kind: "missing" },
      },
      {
        kind: "accountCheck",
        accountId: 1002,
        login: "grace-hopper",
        outcome: { kind: "failed", error: "GitHub answered 502" },
      },
      {
        kind: "accountCheck",
        accountId: 1004,
        login: "retired-account",
        outcome: {
          kind: "interrupted",
          error: "the run stopped before finishing this month: interrupted",
        },
      },
      {
        kind: "orgCheck",
        outcome: { kind: "beforeSettled", checkedAt: "2026-10-01T06:00:00Z" },
      },
    ],
    waiver: null,
  },
  {
    id: "dispositions",
    status: "failed",
    problems: [
      {
        kind: "unlinkedCredits",
        accountId: 2002,
        login: "kj-contractor",
        credits: "120.00",
      },
    ],
    waiver: null,
  },
  {
    id: "unknownAccount",
    status: "failed",
    problems: [
      { kind: "residualNotSettled", reasons: ["notSettled"] },
      { kind: "residual", day: "2026-09-10", credits: "-5.00" },
    ],
    waiver: null,
  },
  {
    id: "repositories",
    status: "failed",
    problems: [
      { kind: "unmappedRepository", repository: "gadget-web", credits: "8.00" },
      { kind: "unrecognizedRepositoryRow" },
    ],
    waiver: null,
  },
  {
    id: "pricing",
    status: "failed",
    problems: [
      {
        kind: "meteredPricing",
        accountId: 1001,
        login: "ada-lovelace",
        reasons: ["oldPricingBasis", "provisionalModel"],
      },
      {
        kind: "oldBasis",
        ...machine,
        provider: "claude",
        buckets: 12,
        firstDay: "2026-09-03",
        lastDay: "2026-09-09",
        estimateUsd: "4.20",
      },
    ],
    waiver: null,
  },
  {
    id: "exchangeRates",
    status: "failed",
    problems: [
      { kind: "rateMissing", currency: "EUR" },
      { kind: "ratePending", currency: "GBP" },
      {
        kind: "rateProvisional",
        currency: "USD",
        observedThrough: "2026-09-29",
      },
      { kind: "adjustedRateMissing", month: "2026-08", currency: "EUR" },
      { kind: "adjustedRatePending", month: "2026-07", currency: "GBP" },
      { kind: "adjustedRateProvisional", month: "2026-08", currency: "USD" },
    ],
    waiver: null,
  },
  {
    id: "uploads",
    status: "failed",
    problems: [
      {
        kind: "machineIncomplete",
        ...machine,
        gaps: [{ from: "2026-09-14", to: "2026-09-15" }],
        providers: [
          {
            provider: "codex",
            gaps: [{ from: "2026-09-14", to: "2026-09-14" }],
          },
        ],
      },
      {
        kind: "utcHoursNotUploaded",
        ...machine,
        provider: null,
        from: "2026-09-30T22:00:00Z",
        until: "2026-10-01T00:00:00Z",
      },
    ],
    waiver: null,
  },
];

const readiness = {
  evaluatedAt: "2026-10-05T08:00:00.5Z",
  ready: false,
  checks,
};

const open = {
  month: "2026-09",
  state: "open",
  timeZone: "Europe/Stockholm",
  overAt: null,
  readiness,
  adjusts: [{ month: "2026-08", snapshotId: 6 }],
  close: null,
  history: [
    {
      closeId: 5,
      closedAt: "2026-10-04T08:00:00Z",
      closedBy: "Grace Hopper",
      reopenedAt: "2026-10-04T09:00:00Z",
      reopenedBy: null,
    },
  ],
};

/** Every problem but a running import, which no waiver covers. */
const withoutRunningImport = (check) => ({
  ...check,
  problems: check.problems.filter(
    (problem) => problem.kind !== "importRunning",
  ),
});

/** The same checks, each failed one waived (once the import finished). */
const waived = {
  ...readiness,
  ready: true,
  checks: checks.map((check) => ({
    ...withoutRunningImport(check),
    status: "waived",
    waiver: { reason: "A retired machine that can never upload again." },
  })),
};

const closed = {
  ...open,
  state: "closed",
  readiness: null,
  adjusts: [],
  close: {
    closeId: 8,
    closedAt: "2026-10-05T08:01:00Z",
    closedBy: null,
    readiness: waived,
    adjustmentsPostOn: "2026-10",
    heldBy: ["2026-10"],
  },
  history: [
    {
      closeId: 8,
      closedAt: "2026-10-05T08:01:00Z",
      closedBy: null,
      reopenedAt: null,
      reopenedBy: null,
    },
    ...open.history,
  ],
};

describe("month close status contract", () => {
  test("parses an open month with every kind of problem", () => {
    expect(parseAiMonthCloseStatus(open)).toEqual(open);
    expect(checks.map((check) => check.id)).toEqual([...CLOSE_CHECKS]);
  });

  test("parses a month in progress", () => {
    const input = {
      ...open,
      month: "2026-10",
      overAt: "2026-11-01T00:00:00Z",
      readiness: null,
      adjusts: [],
      history: [],
    };

    expect(parseAiMonthCloseStatus(input).overAt).toBe("2026-11-01T00:00:00Z");
  });

  test("parses a closed month with its waived readiness", () => {
    expect(parseAiMonthCloseStatus(closed)).toEqual(closed);
  });

  /** The open month, its readiness changed by `changes`. */
  const openWith = (changes) => ({
    ...open,
    readiness: { ...readiness, ...changes },
  });
  /** The closed month, its close's readiness changed by `changes`. */
  const closedWith = (changes) => ({
    ...closed,
    close: { ...closed.close, readiness: { ...waived, ...changes } },
  });

  test.each([
    {
      name: "a waived check without a waiver",
      input: closedWith({ checks: [{ ...waived.checks[0], waiver: null }] }),
    },
    {
      name: "a waiver on a failed check",
      input: openWith({
        checks: [{ ...checks[0], waiver: { reason: "Because." } }],
      }),
    },
    { name: "ready with a failed check", input: openWith({ ready: true }) },
    {
      name: "not ready without a failed check",
      input: closedWith({ ready: false }),
    },
    {
      name: "a check twice",
      input: openWith({ checks: [checks[0], checks[0]] }),
    },
    {
      name: "an unknown check",
      input: openWith({ checks: [{ ...checks[0], id: "seats" }] }),
    },
    {
      name: "an unknown problem",
      input: openWith({
        checks: [{ ...checks[0], problems: [{ kind: "late" }] }],
      }),
    },
    {
      name: "an unknown key on a problem",
      input: openWith({
        checks: [
          {
            ...checks[3],
            problems: [{ kind: "unrecognizedRepositoryRow", sku: "x" }],
          },
        ],
      }),
    },
    {
      name: "an open month with a close",
      input: { ...open, close: closed.close },
    },
    {
      name: "a closed month with live readiness",
      input: { ...closed, readiness },
    },
    {
      name: "an open month both in progress and with readiness",
      input: { ...open, overAt: "2026-10-01T00:00:00Z" },
    },
    {
      name: "an open month neither in progress nor with readiness",
      input: { ...open, readiness: null },
    },
    {
      name: "a closed month that adjusts others",
      input: { ...closed, adjusts: open.adjusts },
    },
  ])("rejects $name", ({ input }) => {
    expect(() => parseAiMonthCloseStatus(input)).toThrow();
  });
});

describe("refused closes", () => {
  test("a month that is not ready answers with its readiness", () => {
    expect(
      parseAiCloseConflict({
        error: "2026-09 is not ready to close",
        readiness,
      }),
    ).toEqual({
      kind: "notReady",
      error: "2026-09 is not ready to close",
      readiness,
    });
  });

  test("a re-open held by adjustments names the months holding them", () => {
    expect(
      parseAiCloseConflict({
        error: "adjustments to 2026-09 are frozen",
        heldBy: ["2026-10"],
      }),
    ).toEqual({
      kind: "heldBy",
      error: "adjustments to 2026-09 are frozen",
      heldBy: ["2026-10"],
    });
  });

  test("any other conflict is only its message", () => {
    expect(
      parseAiCloseConflict({ error: "2026-09 is already closed" }),
    ).toEqual({ kind: "conflict", error: "2026-09 is already closed" });
    expect(parseAiCloseConflict({ message: "no" })).toBeNull();
    expect(
      parseAiCloseConflict({ error: "x", readiness: { ready: true } }),
    ).toBeNull();
  });
});

describe("what a waiver can let through", () => {
  test("a running import cannot be waived, whatever else its check found", () => {
    const [importCheck, ...others] = checks;

    expect(importRunningSince(readiness)).toBe("2026-10-05T07:58:00Z");
    expect(checkWaivable(importCheck)).toBe(false);
    expect(others.every(checkWaivable)).toBe(true);
  });

  test("once the import finishes, its check can be waived", () => {
    const finished = withoutRunningImport(checks[0]);

    expect(checkWaivable(finished)).toBe(true);
    expect(importRunningSince({ ...readiness, checks: [finished] })).toBe(null);
    expect(checkWaivable({ ...finished, status: "passed", problems: [] })).toBe(
      false,
    );
  });
});

describe("waiver reasons", () => {
  test.each([
    { reason: "A retired machine.", ok: true },
    { reason: "  Two lines\nof reason  ", ok: true },
    { reason: "x".repeat(500), ok: true },
    // A code point outside the BMP counts once, as the server counts it.
    { reason: "🙂".repeat(500), ok: true },
    { reason: "   ", ok: false },
    { reason: "x".repeat(501), ok: false },
    { reason: "tab\there", ok: false },
    { reason: "carriage\rreturn", ok: false },
  ])("$reason is acceptable: $ok", ({ reason, ok }) => {
    expect(waiverReasonProblem(reason) === null).toBe(ok);
  });
});
