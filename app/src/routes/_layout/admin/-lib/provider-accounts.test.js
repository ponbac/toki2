import { describe, expect, test } from "bun:test";
import {
  acceptAllPicks,
  clientLabel,
  creditGaps,
  defaultEndDay,
  describeRanges,
  endDayProblem,
  isStale,
  parseDispositionDraft,
  seatSummary,
  targetOptions,
} from "./provider-accounts.ts";

function seat(extra = {}) {
  return {
    createdAt: "2025-09-16T10:00:00+02:00",
    firstDay: "2025-09-16",
    pendingCancellationOn: null,
    lastActivityAt: "2026-10-20T09:00:00Z",
    lastActivityClient: "vscode/1.143.0/copilot-chat/0.71.0",
    listed: true,
    lastDay: null,
    ...extra,
  };
}

function account(accountId, { suggestions = [], dispositions = [] } = {}) {
  return {
    accountId,
    login: `login-${accountId}`,
    profile: null,
    seats: [seat()],
    dispositions,
    currentDispositionId: null,
    uncoveredSeatDays: [],
    creditsThisMonth: "0.00",
    uncoveredCreditDays: [],
    creditDaysWithoutSeat: 0,
    needsLook: dispositions.length === 0,
    suggestions,
    defaultPeriod: { validFrom: "2025-09-16", validTo: null },
  };
}

const verified = (userId) => ({
  userId,
  confidence: "verified",
  evidence: ["name"],
});
const probable = (userId) => ({
  userId,
  confidence: "probable",
  evidence: ["loginFullName"],
});

const ended = {
  id: 9,
  accountId: 3,
  login: "login-3",
  target: { kind: "user", userId: 30 },
  validFrom: "2025-09-16",
  validTo: "2026-09-30",
  updatedAt: "2026-10-01T08:00:00Z",
  updatedBy: 1,
};

describe("acceptAllPicks", () => {
  test("links only verified top candidates of accounts without a disposition", () => {
    const picks = acceptAllPicks([
      account(1, { suggestions: [verified(10), probable(11)] }),
      account(2, { suggestions: [probable(20), verified(21)] }),
      account(3, { suggestions: [verified(30)], dispositions: [ended] }),
      account(4),
    ]);

    expect(picks).toEqual([{ accountId: 1, userId: 10 }]);
  });
});

describe("targetOptions", () => {
  const developers = [
    { userId: 10, fullName: "Ada Andersson", email: "ada@example.com" },
    { userId: 11, fullName: "Bo Berg", email: "bo@example.com" },
    { userId: 12, fullName: "Cleo Carlsson", email: "cleo@example.com" },
  ];

  test("puts suggestions first, then not a Toki user, then everyone else once", () => {
    const options = targetOptions([probable(12), verified(10)], developers);

    expect(options.map((option) => [option.value, option.label])).toEqual([
      ["user:12", "Cleo Carlsson · probable match"],
      ["user:10", "Ada Andersson · verified match"],
      ["external", "Not a Toki user"],
      ["user:11", "Bo Berg"],
    ]);
    expect(options.find((option) => option.value === "user:11")?.keywords).toBe(
      "bo@example.com",
    );
  });
});

describe("parseDispositionDraft", () => {
  const draft = { target: "user:10", validFrom: "2026-10-01", validTo: "" };

  test("reads an ongoing user disposition and a dated external one", () => {
    expect(parseDispositionDraft(draft)).toEqual({
      ok: true,
      terms: {
        target: { kind: "user", userId: 10 },
        validFrom: "2026-10-01",
        validTo: null,
      },
    });
    expect(
      parseDispositionDraft({
        target: "external",
        validFrom: "2026-10-01",
        validTo: "2026-10-01",
      }),
    ).toEqual({
      ok: true,
      terms: {
        target: { kind: "external" },
        validFrom: "2026-10-01",
        validTo: "2026-10-01",
      },
    });
  });

  test.each([
    { name: "no target", input: { ...draft, target: "" } },
    { name: "an unknown target", input: { ...draft, target: "user:0" } },
    { name: "a missing first day", input: { ...draft, validFrom: "" } },
    { name: "an impossible day", input: { ...draft, validFrom: "2026-02-30" } },
    {
      name: "an end before the start",
      input: { ...draft, validTo: "2026-09-30" },
    },
  ])("refuses $name", ({ input }) => {
    expect(parseDispositionDraft(input).ok).toBe(false);
  });
});

describe("the End dialog's last day", () => {
  test("defaults to today, but never before the first day", () => {
    expect(defaultEndDay("2026-10-24", "2026-10-01")).toBe("2026-10-24");
    expect(defaultEndDay("2026-10-24", "2026-11-01")).toBe("2026-11-01");
  });

  test("refuses a day before the first, and accepts the first itself", () => {
    expect(endDayProblem("2026-09-30", "2026-10-01")).not.toBeNull();
    expect(endDayProblem("2026-10-01", "2026-10-01")).toBeNull();
    expect(endDayProblem("2026-10-1", "2026-10-01")).not.toBeNull();
  });
});

describe("seatSummary", () => {
  test("describes the latest seat and counts earlier ones", () => {
    expect(seatSummary([])).toMatchObject({ label: "No seat", earlier: 0 });
    expect(
      seatSummary([seat({ pendingCancellationOn: "2026-10-31" })]),
    ).toMatchObject({
      label: "Since 2025-09-16",
      cancelsOn: "2026-10-31",
      ended: false,
      earlier: 0,
    });
    expect(
      seatSummary([
        seat({ listed: false, lastDay: "2025-12-31" }),
        seat({
          firstDay: "2026-03-02",
          listed: false,
          lastDay: "2026-10-31",
        }),
      ]),
    ).toMatchObject({ label: "Ended 2026-10-31", ended: true, earlier: 1 });
  });
});

describe("clientLabel", () => {
  test.each([
    ["vscode/1.143.0/copilot-chat/0.71.0", "VS Code"],
    [
      "VS/VisualStudio.18.Release/18.10.1/copilot-vs-chat/18.10.1",
      "Visual Studio",
    ],
    ["VisualStudio/17.14.16/copilot-vs/17.14.1231.31060", "Visual Studio"],
    [
      "VS/SSMS.22.SSMS.Release/18.10.1/copilot-vs-chat/18.10.1",
      "SQL Server Management Studio",
    ],
    ["Neovim/0.12.5/copilot.lua/1.553.0", "Neovim"],
    ["copilot-cli", "Copilot CLI"],
    ["unknown/GithubCopilot/1.530.0", "GithubCopilot"],
    ["SomeEditor/2.0", "SomeEditor"],
  ])("%s reads as %s", (client, label) => {
    expect(clientLabel(client)).toBe(label);
  });

  test("keeps a missing client missing", () => {
    expect(clientLabel(null)).toBeNull();
    expect(clientLabel("  ")).toBeNull();
  });
});

describe("describeRanges", () => {
  test("counts both ends across a month boundary", () => {
    expect(
      describeRanges([
        { from: "2026-09-29", through: "2026-10-02" },
        { from: "2026-10-24", through: "2026-10-24" },
      ]),
    ).toBe("Sep 29 – Oct 2, Oct 24 (5 days)");
  });
});

describe("isStale", () => {
  test("flags a last read more than two UTC days before today", () => {
    expect(isStale(null, "2026-10-24")).toBe(false);
    expect(isStale("2026-10-22T23:30:00+02:00", "2026-10-24")).toBe(false);
    expect(isStale("2026-10-21T23:59:00Z", "2026-10-24")).toBe(true);
  });
});

describe("creditGaps", () => {
  test("is empty when every credit day has a disposition and a seat", () => {
    expect(creditGaps(account(1))).toEqual([]);
  });

  test("names uncovered credit days and days outside every seat", () => {
    expect(
      creditGaps({
        ...account(1),
        uncoveredCreditDays: [
          { from: "2026-10-03", through: "2026-10-04" },
          { from: "2026-10-09", through: "2026-10-09" },
        ],
        creditDaysWithoutSeat: 1,
      }),
    ).toEqual([
      "Credits without a disposition: Oct 3 – Oct 4, Oct 9 (3 days)",
      "1 day with credits outside every seat",
    ]);
  });
});
