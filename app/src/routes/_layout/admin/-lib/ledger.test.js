import { describe, expect, test } from "bun:test";
import { allocationSegments, ledgerOf, splitProjectName } from "./ledger.ts";

const usage = {
  apiEquivalentUsd: 1,
  unpricedRecords: 0,
  records: 1,
  tokens: { input: 1, cacheRead: 0, cacheWrite: 0, output: 1, total: 2 },
};

function line(projectName, sek, extra = {}) {
  return {
    userId: 1,
    provider: "codex",
    project: projectName ? { projectId: projectName, projectName } : null,
    billingMode: "api",
    unallocatedOverhead: false,
    subscriptionId: null,
    billableAmount: sek,
    billableCurrency: "SEK",
    convertedAmount: sek,
    exchangeRate: null,
    missingRate: false,
    usage,
    ...extra,
  };
}

describe("ledgerOf", () => {
  test("ranks projects dearest first and keeps Unassigned and overhead apart", () => {
    const ledger = ledgerOf([
      line("Small - A", "10.00"),
      line(null, "500.00"),
      line("Big - B", "300.00"),
      line("Big - B", "200.00", { userId: 2 }),
      line("Unpriced - C", null, { convertedAmount: null }),
      line(null, "50.00", {
        unallocatedOverhead: true,
        billingMode: "subscription",
      }),
    ]);

    expect(ledger.projects.map((e) => e.label)).toEqual([
      "Big - B",
      "Small - A",
      "Unpriced - C",
    ]);
    expect(ledger.projects[0].totals.converted).toBe(50000);
    expect(ledger.projects[0].developers).toEqual([1, 2]);
    expect(ledger.unassigned?.totals.converted).toBe(50000);
    expect(ledger.overhead?.totals.converted).toBe(5000);
  });
});

describe("allocationSegments", () => {
  test("merges two or more small projects and leaves out what bills nothing", () => {
    const ledger = ledgerOf([
      line("Big - B", "9000.00"),
      line("Tiny - T", "10.00"),
      line("Wee - W", "20.00"),
      line("Free - F", "0.00"),
      line(null, "970.00"),
    ]);

    expect(
      allocationSegments(ledger).map((s) => [s.kind, s.label, s.amount]),
    ).toEqual([
      ["project", "Big - B", 900000],
      ["others", "2 smaller projects", 3000],
      ["unassigned", "Unassigned", 97000],
    ]);
  });

  test("a single small project keeps its own segment", () => {
    const ledger = ledgerOf([
      line("Big - B", "9900.00"),
      line("Tiny - T", "10.00"),
    ]);

    expect(allocationSegments(ledger).map((s) => s.kind)).toEqual([
      "project",
      "project",
    ]);
  });
});

describe("splitProjectName", () => {
  test.each([
    ["Lerums Djursjukhus - Apport", "Lerums Djursjukhus", "Apport"],
    [
      "Ex-Change Parts - Infrastructure Hexagon",
      "Ex-Change Parts",
      "Infrastructure Hexagon",
    ],
    ["Customer - Phase 1 - Build", "Customer", "Phase 1 - Build"],
    ["DS Smith Maintenance", "DS Smith Maintenance", null],
    [" - Orphan", " - Orphan", null],
  ])("%s", (name, customer, project) => {
    expect(splitProjectName(name)).toEqual({ customer, project });
  });
});
