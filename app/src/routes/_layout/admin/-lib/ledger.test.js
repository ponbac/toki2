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
  ["Norrsken Logistik - Kundportal", "6454.93"],
  ["Tallbo Fastighet - Underhåll", "1901.18"],
  ["Ljusdal Energi - Driftstöd", "1532.07"],
  ["Bo-Ek Bygg - Offertverktyg", "1259.38"],
  ["Glimmer Retail - Butiksapp", "629.85"],
  ["Stensjö Vård - Bokning", "246.17"],
  ["Hamnverket - Integrationer", "210.75"],
  ["Kvarnens Mat - E-handel", "135.42"],
  ["Ekdal Media - Webbplats", "8.72"],
  [null, "6880.84"],
];

const SEPTEMBER = [
  ["Norrsken Logistik - Kundportal", "8755.80"],
  ["Ljusdal Energi - Driftstöd", "1021.80"],
  ["Tallbo Fastighet - Underhåll", "784.30"],
  ["Glimmer Retail - Butiksapp", "372.49"],
  ["Stensjö Vård - Bokning", "328.65"],
  ["Kvarnens Mat - E-handel", "142.98"],
  ["Hamnverket - Integrationer", "59.31"],
  ["Nordhamn Drift", "8.39"],
  [null, "7588.09"],
];

describe("allocationSegments", () => {
  test("October: four projects of their own, the rest named after the dearest", () => {
    expect(steps(allocationSegments(month(OCTOBER)))).toEqual([
      ["project", "Norrsken Logistik - Kundportal", 1, 1],
      ["project", "Tallbo Fastighet - Underhåll", 2, 1],
      ["project", "Ljusdal Energi - Driftstöd", 3, 1],
      ["project", "Bo-Ek Bygg - Offertverktyg", 4, 1],
      // Glimmer Retail is 3% of the bill: merged by the cap, not by its size.
      ["others", "Glimmer Retail + 4 more", 5, 5],
      ["unassigned", "Unassigned", null, 1],
    ]);
  });

  test("September: a tail merged by the cap is named after its dearest", () => {
    expect(steps(allocationSegments(month(SEPTEMBER)))).toEqual([
      ["project", "Norrsken Logistik - Kundportal", 1, 1],
      ["project", "Ljusdal Energi - Driftstöd", 2, 1],
      ["project", "Tallbo Fastighet - Underhåll", 3, 1],
      ["project", "Glimmer Retail - Butiksapp", 4, 1],
      ["others", "Stensjö Vård + 3 more", 5, 4],
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

describe("allocationSegments without the whole bill", () => {
  test("a line without an exchange rate leaves no strip, so no share reads as the whole bill", () => {
    const ledger = ledgerOf([
      line("Big - B", "900.00"),
      line("Foreign - F", null, { missingRate: true, convertedAmount: null }),
    ]);
    const segments = allocationSegments(ledger);

    expect(ledger.totals.converted).toBeNull();
    expect(segments).toEqual([]);
    expect(segmentSteps(segments).size).toBe(0);
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
    expect(map.get("project:Glimmer Retail - Butiksapp")).toBe(5);
    expect(map.get("project:Ekdal Media - Webbplats")).toBe(5);
    expect(map.has("unassigned")).toBe(false);
    expect(map.has("overhead")).toBe(false);
    expect(map.has("project:Unpriced - U")).toBe(false);
  });
});

describe("splitProjectName", () => {
  test.each([
    ["Norrsken Logistik - Kundportal", "Norrsken Logistik", "Kundportal"],
    ["Bo-Ek Bygg - Offertverktyg", "Bo-Ek Bygg", "Offertverktyg"],
    ["Customer - Phase 1 - Build", "Customer", "Phase 1 - Build"],
    ["Nordhamn Drift", "Nordhamn Drift", null],
    [" - Orphan", " - Orphan", null],
  ])("%s", (name, customer, project) => {
    expect(splitProjectName(name)).toEqual({ customer, project });
  });
});
