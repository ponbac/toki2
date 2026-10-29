import { describe, expect, test } from "bun:test";
import { machineFindings } from "./machine-findings.ts";

const coverage = {
  provider: "copilot",
  status: "ok",
  files: 3,
  unreadable: 0,
  malformedLines: 0,
  skippedRecords: 0,
  duplicates: 0,
  windowStart: "2026-10-21T22:00:00Z",
  windowEnd: "2026-10-22T22:00:00Z",
  reportedAt: "2026-10-22T22:00:00Z",
  pricing: {
    status: "unavailable",
    fetchedAt: null,
    source: "https://example.com/prices.json",
  },
  hasUsage: true,
};

function machine(cost, read = coverage) {
  return {
    machineId: "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a",
    label: "work-laptop",
    clientVersion: "0.7.0",
    timeZone: "Europe/Stockholm",
    firstSeenAt: "2026-10-01T08:00:00Z",
    lastSyncedAt: "2026-10-22T22:00:00Z",
    stale: false,
    usage: {
      tokens: { input: 1200, cacheRead: 0, cacheWrite: 0, output: 900 },
      records: 374,
      cost,
    },
    coverage: [read],
  };
}

const findings = (cost) => machineFindings(machine(cost), 7);

describe("machine findings", () => {
  test("unpriced usage is a warning that names missing rates as a cause", () => {
    expect(
      findings({ kind: "partial", knownUsd: 0, unpricedRecords: 374 }),
    ).toEqual([
      {
        key: "unpriced",
        severity: "warning",
        text: "374 of its requests in these dates have no known price, so their cost is unknown. Prices were unavailable when it last synced.",
      },
    ]);
  });

  test("missing rates are no warning when every request has a price", () => {
    // Copilot's next edit suggestions cost nothing without any rates.
    expect(findings({ kind: "estimated", usd: 0 })).toEqual([]);
  });

  test("records skipped on a partial read are no finding", () => {
    const partial = {
      ...coverage,
      status: "partial",
      malformedLines: 2,
      skippedRecords: 40,
      pricing: null,
    };
    expect(
      machineFindings(machine({ kind: "estimated", usd: 12.5 }, partial), 7),
    ).toEqual([]);
  });
});
