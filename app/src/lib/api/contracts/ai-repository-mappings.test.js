import { describe, expect, test } from "bun:test";
import {
  parseAiRepositoryMapping,
  parseAiRepositoryMappings,
} from "./ai-repository-mappings.ts";

const clientA = { projectId: "101", projectName: "Client A" };

/** A saved mapping as the backend serializes it. */
const mapping = {
  repository: "widget-api",
  project: clientA,
  status: "resolves",
  createdBy: { userId: 3, fullName: "Ada Lovelace" },
  createdAt: "2026-10-25T10:00:00.123456Z",
  updatedBy: null,
  updatedAt: "2026-10-25T10:00:00Z",
};

/** Credits in September and October; October, the cutover month, bills
 * unless it has none. */
function months(september, october) {
  return [
    { month: "2026-09", credits: september, billed: false },
    { month: "2026-10", credits: october, billed: october !== "0.00" },
  ];
}

/** The overview as the backend serializes it. */
const overview = {
  provider: "copilot",
  billedFrom: "2026-10",
  months: ["2026-09", "2026-10"],
  repositories: [
    {
      repository: "gadget-web",
      months: months("0.00", "800.00"),
      lastBilledOn: "2026-10-20",
      mapping: null,
      suggestions: [],
    },
    {
      repository: "widget-api",
      months: months("100.00", "1200.00"),
      lastBilledOn: "2026-10-24",
      mapping: null,
      suggestions: [
        {
          project: clientA,
          projectKeys: ["github.com/example-org/widget-api"],
        },
      ],
    },
    {
      repository: "Old-Service",
      months: months("0.00", "0.00"),
      lastBilledOn: null,
      mapping: { ...mapping, repository: "Old-Service", status: "stale" },
      suggestions: [],
    },
  ],
};

/** The overview with its first repository replaced by `row`. */
function withRow(row) {
  return {
    ...overview,
    repositories: [{ ...overview.repositories[0], ...row }],
  };
}

describe("repository mappings contract", () => {
  test("parses the overview the backend serializes", () => {
    expect(parseAiRepositoryMappings(overview)).toEqual(overview);
  });

  test("parses an overview without a cutover or repositories", () => {
    const empty = { ...overview, billedFrom: null, repositories: [] };
    expect(parseAiRepositoryMappings(empty)).toEqual(empty);
  });

  test("parses months that bill nothing without a cutover", () => {
    const uncut = withRow({
      months: [
        { month: "2026-09", credits: "100.00", billed: false },
        { month: "2026-10", credits: "800.00", billed: false },
      ],
    });
    const parsed = { ...uncut, billedFrom: null };
    expect(parseAiRepositoryMappings(parsed)).toEqual(parsed);
  });

  test("parses a saved mapping", () => {
    expect(parseAiRepositoryMapping(mapping)).toEqual(mapping);
  });

  test.each([
    {
      name: "a resolving mapping with suggestions",
      input: withRow({
        mapping,
        suggestions: overview.repositories[1].suggestions,
      }),
    },
    {
      name: "a repository neither billed nor mapped",
      input: withRow({ lastBilledOn: null }),
    },
    {
      name: "a row whose months are not the overview's",
      input: withRow({
        months: [{ month: "2026-10", credits: "800.00", billed: true }],
      }),
    },
    {
      name: "a month without whether it bills",
      input: withRow({
        months: months("0.00", "800.00").map(({ month, credits }) => ({
          month,
          credits,
        })),
      }),
    },
    {
      name: "a billed month before the cutover",
      input: withRow({
        months: [
          { month: "2026-09", credits: "100.00", billed: true },
          { month: "2026-10", credits: "800.00", billed: true },
        ],
      }),
    },
    {
      name: "a billed month without a cutover",
      input: { ...overview, billedFrom: null },
    },
    {
      name: "a row whose months are out of order",
      input: withRow({ months: months("800.00", "0.00").reverse() }),
    },
    {
      name: "a suggestion without project keys",
      input: withRow({ suggestions: [{ project: clientA, projectKeys: [] }] }),
    },
    {
      name: "an empty repository name",
      input: withRow({ repository: "" }),
    },
    {
      name: "credits without hundredths",
      input: withRow({ months: months("0", "800") }),
    },
    {
      name: "an unknown mapping status",
      input: withRow({ mapping: { ...mapping, status: "pending" } }),
    },
    {
      name: "an unknown key on a row",
      input: withRow({ project: clientA }),
    },
    {
      name: "an unknown key on the overview",
      input: { ...overview, unmappedRepositories: 1 },
    },
  ])("rejects $name", ({ input }) => {
    expect(() => parseAiRepositoryMappings(input)).toThrow();
  });

  test("rejects a mapping with an unknown key", () => {
    expect(() =>
      parseAiRepositoryMapping({ ...mapping, projectKey: "widget-api" }),
    ).toThrow();
  });
});
