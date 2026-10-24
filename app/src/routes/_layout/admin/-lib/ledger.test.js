import { describe, expect, test } from "bun:test";
import {
  allocationSegments,
  ledgerOf,
  segmentSteps,
  splitProjectName,
} from "./ledger.ts";

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

/** A month's ledger from `[name, sek]` pairs; a null name is Unassigned. */
function month(rows) {
  return ledgerOf(rows.map(([name, sek]) => line(name, sek)));
}

const steps = (segments) =>
  segments.map((s) => [s.kind, s.label, s.step, s.entries.length]);

const OCTOBER = [
  ["Lerums Djursjukhus - Apport", "6454.93"],
  ["Rentalföretagen - Rentalkod 2.0", "1901.18"],
  ["Ecogain - AI project", "1532.07"],
  ["Ex-Change Parts - Infrastructure Hexagon", "1259.38"],
  ["ChildX - Widget", "629.85"],
  ["Scartab - Self-billing", "246.17"],
  ["Nordicon - Inläsning av mail", "210.75"],
  ["Ramirent - Ramiconnect", "135.42"],
  ["Blixt Biltvättar - Webbplats", "8.72"],
  [null, "6880.84"],
];

const SEPTEMBER = [
  ["Lerums Djursjukhus - Apport", "8755.80"],
  ["Ecogain - AI project", "1021.80"],
  ["Rentalföretagen - Rentalkod 2.0", "784.30"],
  ["ChildX - Widget", "372.49"],
  ["Scartab - Self-billing", "328.65"],
  ["Ramirent - Ramiconnect", "142.98"],
  ["Nordicon - Inläsning av mail", "59.31"],
  ["DS Smith Maintenance", "8.39"],
  [null, "7588.09"],
];

describe("allocationSegments", () => {
  test("October: four projects of their own, the rest named after the dearest", () => {
    expect(steps(allocationSegments(month(OCTOBER)))).toEqual([
      ["project", "Lerums Djursjukhus - Apport", 1, 1],
      ["project", "Rentalföretagen - Rentalkod 2.0", 2, 1],
      ["project", "Ecogain - AI project", 3, 1],
      ["project", "Ex-Change Parts - Infrastructure Hexagon", 4, 1],
      // ChildX is 3% of the bill: merged by the cap, not by its size.
      ["others", "ChildX + 4 more", 5, 5],
      ["unassigned", "Unassigned", null, 1],
    ]);
  });

  test("September: a tail merged by the cap is named after its dearest", () => {
    expect(steps(allocationSegments(month(SEPTEMBER)))).toEqual([
      ["project", "Lerums Djursjukhus - Apport", 1, 1],
      ["project", "Ecogain - AI project", 2, 1],
      ["project", "Rentalföretagen - Rentalkod 2.0", 3, 1],
      ["project", "ChildX - Widget", 4, 1],
      ["others", "Scartab + 3 more", 5, 4],
      ["unassigned", "Unassigned", null, 1],
    ]);
  });

  test("a tail of only small projects says so, and keeps step 5 after fewer than four", () => {
    const segments = allocationSegments(
      month([
        ["Big - B", "6000.00"],
        ["Mid - M", "3700.00"],
        ["Tiny - T", "100.00"],
        ["Wee - W", "100.00"],
        ["Mini - I", "100.00"],
        ["Free - F", "0.00"],
      ]),
    );

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["project", "Mid - M", 2, 1],
      ["others", "3 smaller projects", 5, 3],
    ]);
    expect(segments[2].amount).toBe(30000);
  });

  test("a single project left over keeps its own segment on the tail's step", () => {
    const segments = allocationSegments(
      month([
        ["Big - B", "9900.00"],
        ["Tiny - T", "10.00"],
      ]),
    );

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["project", "Tiny - T", 5, 1],
    ]);
    expect(segments[1].key).toBe("project:Tiny - T");
  });

  test("thirty projects still give four of their own and one tail, stepped without gaps", () => {
    const rows = Array.from({ length: 30 }, (_, index) => [
      `Customer ${index} - Project`,
      index < 6 ? `${1000 - index * 10}.00` : "10.00",
    ]);
    const segments = allocationSegments(month(rows));
    const stepped = segments.map((s) => s.step);

    expect(stepped).toEqual([1, 2, 3, 4, 5]);
    expect(segments[4].entries).toHaveLength(26);
    expect(segments[4].label).toBe("Customer 4 + 25 more");
  });
});

describe("segmentSteps", () => {
  test("every row maps to its segment's step; Unassigned, overhead and unknown amounts do not", () => {
    const ledger = ledgerOf([
      ...OCTOBER.map(([name, sek]) => line(name, sek)),
      line("Unpriced - U", null, { convertedAmount: null }),
      line(null, "50.00", {
        unallocatedOverhead: true,
        billingMode: "subscription",
      }),
    ]);
    const segments = allocationSegments(ledger);
    const map = segmentSteps(segments);

    for (const segment of segments) {
      for (const entry of segment.entries) {
        expect(map.get(entry.key) ?? null).toBe(segment.step);
      }
    }
    expect(map.get("project:ChildX - Widget")).toBe(5);
    expect(map.get("project:Blixt Biltvättar - Webbplats")).toBe(5);
    expect(map.has("unassigned")).toBe(false);
    expect(map.has("overhead")).toBe(false);
    expect(map.has("project:Unpriced - U")).toBe(false);
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
