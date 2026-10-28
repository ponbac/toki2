import { describe, expect, test } from "bun:test";
import {
  activeSuggestions,
  repositoryNeedsLook,
  repositoryStatus,
  suggestionTitle,
  unbilledCreditsReason,
} from "./repository-mappings.ts";

const clientA = { projectId: "101", projectName: "Client A" };

/** A month's credits as the server marks them under an October cutover: a
 * month bills from October on once its credits round to a credit. */
function monthCredits(month, credits) {
  const [units, hundredths] = credits.split(".").map(Number);
  return {
    month,
    credits,
    billed: month >= "2026-10" && units * 100 + hundredths >= 50,
  };
}

/** A repository with `september` and `october` credits and no mapping. */
function row(september, october, mapping = null) {
  return {
    repository: "widget-api",
    months: [
      monthCredits("2026-09", september),
      monthCredits("2026-10", october),
    ],
    lastBilledOn: "2026-10-24",
    mapping,
    suggestions: [],
  };
}

function mapping(status) {
  return {
    repository: "widget-api",
    project: clientA,
    status,
    createdBy: null,
    createdAt: "2026-10-25T10:00:00Z",
    updatedBy: null,
    updatedAt: "2026-10-25T10:00:00Z",
  };
}

describe("repositoryStatus", () => {
  test("is the mapping's status, or unmapped without one", () => {
    expect(repositoryStatus(row("0.00", "1.00"))).toBe("unmapped");
    for (const status of ["resolves", "stale", "unconfigured"]) {
      expect(repositoryStatus(row("0.00", "1.00", mapping(status)))).toBe(
        status,
      );
    }
  });
});

describe("repositoryNeedsLook", () => {
  test("flags a billed month that bills as Unassigned", () => {
    expect(repositoryNeedsLook(row("0.00", "0.50"))).toBe(true);
    expect(repositoryNeedsLook(row("0.00", "12.00", mapping("stale")))).toBe(
      true,
    );
    expect(
      repositoryNeedsLook(row("0.00", "12.00", mapping("unconfigured"))),
    ).toBe(true);
  });

  test("passes a resolving mapping and a repository without recent credits", () => {
    expect(
      repositoryNeedsLook(row("100.00", "12.00", mapping("resolves"))),
    ).toBe(false);
    expect(repositoryNeedsLook(row("0.00", "0.00"))).toBe(false);
  });

  test("passes credits that bill no line: before the cutover or under a credit", () => {
    // Only in September, before the cutover: the bill has no line for it.
    expect(repositoryNeedsLook(row("100.00", "0.00"))).toBe(false);
    // October's credits round to no credit.
    expect(repositoryNeedsLook(row("0.00", "0.40"))).toBe(false);
  });
});

describe("unbilledCreditsReason", () => {
  test("says why credits bill no line", () => {
    expect(
      unbilledCreditsReason(monthCredits("2026-09", "100.00"), "2026-10"),
    ).toBe("Not billed: before the cutover month");
    expect(
      unbilledCreditsReason(monthCredits("2026-10", "0.40"), "2026-10"),
    ).toBe("Not billed: under one credit for the month");
    expect(
      unbilledCreditsReason(
        { month: "2026-10", credits: "12.00", billed: false },
        null,
      ),
    ).toBe("Not billed: no cutover month is configured");
  });

  test("is null for credits that bill a line and for none", () => {
    expect(
      unbilledCreditsReason(monthCredits("2026-10", "12.00"), "2026-10"),
    ).toBeNull();
    expect(
      unbilledCreditsReason(monthCredits("2026-09", "0.00"), "2026-10"),
    ).toBeNull();
  });
});

describe("suggestionTitle", () => {
  test("names the mapped keys behind a suggestion", () => {
    expect(
      suggestionTitle({
        project: clientA,
        projectKeys: ["github.com/example-org/widget-api"],
      }),
    ).toBe("Mapped key: github.com/example-org/widget-api");
    expect(
      suggestionTitle({
        project: clientA,
        projectKeys: [
          "github.com/example-org/widget-api",
          "github.com/example-fork/widget-api",
        ],
      }),
    ).toBe(
      "Mapped keys: github.com/example-org/widget-api, github.com/example-fork/widget-api",
    );
  });
});

describe("activeSuggestions", () => {
  const clientB = { projectId: "202", projectName: "Client B" };
  const suggest = (project) => ({
    project,
    projectKeys: [`github.com/example-org/${project.projectId}`],
  });

  test("keeps only suggestions of an active project", () => {
    // Client B's project is archived: its key mapping still names it.
    expect(
      activeSuggestions([suggest(clientA), suggest(clientB)], [clientA]),
    ).toEqual([suggest(clientA)]);
    expect(activeSuggestions([suggest(clientB)], [clientA])).toEqual([]);
  });

  test("offers none until the active projects are known", () => {
    expect(activeSuggestions([suggest(clientA)], undefined)).toEqual([]);
  });
});
