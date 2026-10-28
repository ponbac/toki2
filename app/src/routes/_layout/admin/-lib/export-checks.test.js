import { describe, expect, test } from "bun:test";
import { mappingsCheck } from "./export-checks.ts";

const usage = {
  apiEquivalentUsd: 1,
  unpricedRecords: 0,
  records: 1,
  tokens: { input: 1, cacheRead: 0, cacheWrite: 0, output: 1, total: 2 },
};

function line(unassigned, sek, extra = {}) {
  return {
    userId: 1,
    provider: "codex",
    project: unassigned ? null : { projectId: "1", projectName: "Client" },
    unassigned,
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

describe("mappingsCheck", () => {
  test("passes when every line has a project", () => {
    expect(mappingsCheck([line(null, "10.00")], "SEK")).toEqual({
      status: "passed",
      summary: "Every billed line has a project",
    });
  });

  test("names what mapping repositories would assign apart from what it cannot", () => {
    const check = mappingsCheck(
      [
        line(null, "10.00"),
        line("unmapped", "20.00"),
        line("unattributed", "300.00"),
        line("unattributed", "5.00", { userId: 2 }),
      ],
      "SEK",
    );

    expect(check.status).toBe("attention");
    expect(check.summary).toMatch(
      /^20,00\sSEK comes from repositories nobody has mapped, so no customer can be invoiced for it yet; 305,00\sSEK is outside any repository, which no mapping can assign$/,
    );
  });

  test("usage outside any repository alone passes: there is nothing to map", () => {
    // A bill without code review, such as a month before the cutover.
    const check = mappingsCheck(
      [line(null, "10.00"), line("unattributed", "300.00")],
      "SEK",
    );

    expect(check.status).toBe("passed");
    expect(check.summary).toMatch(
      /^Every repository is mapped; 300,00\sSEK is outside any repository, which no mapping can assign$/,
    );
  });

  test("names mapped code review only on a bill that has it", () => {
    const check = mappingsCheck(
      [
        line(null, "8.00", {
          subject: {
            kind: "repository",
            provider: "copilot",
            name: "widget-api",
          },
          billingMode: "metered",
          metered: { kind: "repository" },
        }),
        line("unattributed", "300.00"),
      ],
      "SEK",
    );

    expect(check.status).toBe("passed");
    expect(check.summary).toMatch(
      /^Every repository is mapped, GitHub's code review repositories too; 300,00\sSEK is outside any repository, which no mapping can assign$/,
    );
  });

  test("code review of unmapped GitHub repositories needs attention", () => {
    const check = mappingsCheck(
      [
        line(null, "10.00"),
        line("unmappedRepository", "8.00", {
          subject: {
            kind: "repository",
            provider: "copilot",
            name: "gadget-web",
          },
          billingMode: "metered",
        }),
      ],
      "SEK",
    );

    expect(check.status).toBe("attention");
    expect(check.summary).toMatch(
      /^8,00\sSEK of code review comes from GitHub repositories nobody has mapped \(Project mappings > GitHub repositories\)$/,
    );
  });

  test("names every kind of Unassigned usage, in bill order", () => {
    const check = mappingsCheck(
      [
        line("unattributed", "300.00"),
        line("unmappedRepository", "8.00"),
        line("unmapped", "20.00"),
      ],
      "SEK",
    );

    expect(check.status).toBe("attention");
    expect(check.summary).toMatch(
      /^20,00\sSEK comes from repositories nobody has mapped, so no customer can be invoiced for it yet; 8,00\sSEK of code review comes from GitHub repositories nobody has mapped \(Project mappings > GitHub repositories\); 300,00\sSEK is outside any repository, which no mapping can assign$/,
    );
  });

  test("lines of unknown cost are counted, not summed", () => {
    const check = mappingsCheck(
      [
        line("unmapped", null, {
          convertedAmount: null,
          usage: { ...usage, unpricedRecords: 1 },
        }),
      ],
      "SEK",
    );

    expect(check.summary).toBe(
      "1 line of unknown cost comes from repositories nobody has mapped, so no customer can be invoiced for it yet",
    );
  });
});
