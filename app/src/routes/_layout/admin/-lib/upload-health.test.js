import { describe, expect, test } from "bun:test";
import { uploadsCheck } from "./export-checks.ts";
import { monthUploads } from "./upload-health.ts";

function machine({ gaps = [], providers = [], activeInMonth = true } = {}) {
  return {
    machineId: "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a",
    label: "work-laptop",
    clientVersion: "0.9.0",
    lastSyncedOn: "2026-10-23",
    stale: false,
    activeInMonth,
    complete: gaps.length === 0 && providers.every((p) => p.gaps.length === 0),
    gaps,
    providers,
  };
}

function developer(userId, fullName, machines, extra = {}) {
  return {
    userId,
    fullName,
    email: `${userId}@example.com`,
    readiness: "noSync",
    hasUsage: true,
    hasSubscription: false,
    machines,
    ...extra,
  };
}

/** October in progress, required through Thursday Oct 22. */
function october(developers) {
  return {
    month: "2026-10",
    requiredThrough: "2026-10-22",
    inProgress: true,
    staleAfterDays: 7,
    developers,
  };
}

const verdicts = (completeness) =>
  Object.fromEntries(
    monthUploads(completeness).map((u) => [u.developer.fullName, u.verdict]),
  );

describe("monthUploads", () => {
  test("tells a late start, a hole, a whole month and a pending sync apart", () => {
    const month = october([
      developer(1, "Late", [
        machine({ gaps: [{ from: "2026-10-01", to: "2026-10-08" }] }),
      ]),
      developer(2, "Hole", [
        machine({ gaps: [{ from: "2026-10-12", to: "2026-10-13" }] }),
      ]),
      developer(3, "Nothing", [
        machine({ gaps: [{ from: "2026-10-01", to: "2026-10-22" }] }),
      ]),
      developer(4, "Pending", [
        machine({ gaps: [{ from: "2026-10-20", to: "2026-10-22" }] }),
      ]),
      developer(5, "Stalled", [
        machine({ gaps: [{ from: "2026-10-19", to: "2026-10-22" }] }),
      ]),
      developer(6, "Done", [machine()], { readiness: "ready" }),
      developer(7, "Idle", [machine({ activeInMonth: false })]),
      developer(8, "Unsynced", [], { hasSubscription: true }),
    ]);

    expect(verdicts(month)).toEqual({
      Late: "lateStart",
      Hole: "holes",
      Nothing: "nothingUploaded",
      Pending: "awaitingSync",
      // More than a weekend without a sync is a hole, even at the month's end.
      Stalled: "holes",
      Done: "complete",
      Idle: "noActivity",
      Unsynced: "neverSynced",
    });
  });

  test("a gap on one of two machines is the developer's gap", () => {
    const month = october([
      developer(1, "Two machines", [
        machine(),
        machine({ gaps: [{ from: "2026-10-14", to: "2026-10-14" }] }),
      ]),
    ]);
    const [upload] = monthUploads(month);

    expect(upload.verdict).toBe("holes");
    expect(upload.runs).toEqual([
      { from: "2026-10-14", to: "2026-10-14", days: 1, kind: "hole" },
    ]);
  });

  test("days a machine uploaded without an expected tool are partial", () => {
    const month = october([
      developer(1, "Tool", [
        machine({
          providers: [
            {
              provider: "copilot",
              status: "ok",
              pricingStatus: "fresh",
              expected: true,
              gaps: [{ from: "2026-10-05", to: "2026-10-06" }],
            },
          ],
        }),
      ]),
    ]);
    const [upload] = monthUploads(month);

    expect(upload.verdict).toBe("toolGaps");
    expect(upload.toolDays).toBe(2);
    expect(upload.days[4]).toBe("partial");
    expect(upload.days[22]).toBe("notRequired");
  });

  test("a closed month's trailing gap is a hole, not a pending sync", () => {
    const month = {
      ...october([
        developer(1, "Closed", [
          machine({ gaps: [{ from: "2026-10-30", to: "2026-10-31" }] }),
        ]),
      ]),
      requiredThrough: "2026-10-31",
      inProgress: false,
    };

    expect(verdicts(month)).toEqual({ Closed: "holes" });
  });
});

describe("uploadsCheck", () => {
  test("late starts and pending syncs alone pass, and say so", () => {
    const month = october([
      developer(1, "Late", [
        machine({ gaps: [{ from: "2026-10-01", to: "2026-10-08" }] }),
      ]),
      developer(2, "Pending", [
        machine({ gaps: [{ from: "2026-10-22", to: "2026-10-22" }] }),
      ]),
    ]);

    expect(uploadsCheck(month, monthUploads(month))).toEqual({
      status: "passed",
      summary:
        "Everything is in, except days that wait for a sync; 1 started mid-month",
    });
  });

  test("a tool missing after a late start still needs attention", () => {
    const month = october([
      developer(1, "Late and missing a tool", [
        machine({
          gaps: [{ from: "2026-10-01", to: "2026-10-08" }],
          providers: [
            {
              provider: "opencode",
              status: "ok",
              pricingStatus: "fresh",
              expected: true,
              gaps: [{ from: "2026-10-09", to: "2026-10-14" }],
            },
          ],
        }),
      ]),
    ]);
    const uploads = monthUploads(month);

    expect(uploads[0].verdict).toBe("toolGaps");
    expect(uploadsCheck(month, uploads)).toEqual({
      status: "attention",
      summary: "1 of 1 developer misses a tool on some days",
    });
  });

  test("a hole needs attention and is counted apart from tool gaps", () => {
    const month = october([
      developer(1, "Hole", [
        machine({ gaps: [{ from: "2026-10-12", to: "2026-10-13" }] }),
      ]),
      developer(2, "Tool", [
        machine({
          providers: [
            {
              provider: "codex",
              status: "partial",
              pricingStatus: "fresh",
              expected: true,
              gaps: [{ from: "2026-10-05", to: "2026-10-05" }],
            },
          ],
        }),
      ]),
      developer(3, "Late", [
        machine({ gaps: [{ from: "2026-10-01", to: "2026-10-08" }] }),
      ]),
    ]);

    expect(uploadsCheck(month, monthUploads(month))).toEqual({
      status: "attention",
      summary: "2 of 3 developers have gaps; 1 misses days or uploaded nothing",
    });
  });
});
