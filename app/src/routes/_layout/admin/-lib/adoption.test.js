import { describe, expect, test } from "bun:test";
import {
  companyTotals,
  everyone,
  habitOf,
  indexDays,
  previousWindow,
  topShare,
  volumeBars,
  volumeStep,
  windowOf,
} from "./adoption.ts";

function person(userId, fullName, linked = true) {
  return { userId, fullName, linked, firstSyncedOn: null, seats: [] };
}

function day(userId, date, tool, costUsd, tokens = 100) {
  return {
    userId,
    day: date,
    tool,
    costUsd,
    unpricedRecords: 0,
    records: 1,
    tokens,
  };
}

/** Two weeks to Wednesday 28 October 2026. */
const ADOPTION = {
  today: "2026-10-28",
  timeZone: "Europe/Stockholm",
  from: "2026-09-01",
  to: "2026-10-28",
  currency: "SEK",
  credits: null,
  people: [person(1, "Ada"), person(2, "Ben"), person(3, "Cy")],
  days: [
    // Ada: every work day of the last week, on two tools on Monday.
    day(1, "2026-10-26", "codex", 10),
    day(1, "2026-10-26", "copilot", 1),
    day(1, "2026-10-27", "codex", 20),
    day(1, "2026-10-28", "codex", 30),
    // A Saturday counts as a day with use, not as a work day.
    day(1, "2026-10-24", "claude", 5),
    // Ben: one work day.
    day(2, "2026-10-22", "copilot", 2),
  ],
  companyDays: [{ day: "2026-10-27", tool: "copilot", costUsd: 4 }],
};

describe("windows", () => {
  test("end today and reach back whole weeks", () => {
    expect(windowOf("2026-10-28", 1)).toEqual({
      from: "2026-10-22",
      to: "2026-10-28",
    });
    expect(previousWindow({ from: "2026-10-22", to: "2026-10-28" })).toEqual({
      from: "2026-10-15",
      to: "2026-10-21",
    });
  });
});

describe("everyone", () => {
  const range = windowOf("2026-10-28", 1);
  const stats = everyone(ADOPTION, indexDays(ADOPTION.days), range);

  test("is ordered by cost, with people without use last", () => {
    expect(stats.map((entry) => entry.person.fullName)).toEqual([
      "Ada",
      "Ben",
      "Cy",
    ]);
  });

  test("counts work days and days with use apart", () => {
    const [ada, ben, cy] = stats;
    // 22–28 October holds five work days: Thu, Fri, Mon, Tue, Wed.
    expect(ada.workDays).toBe(5);
    expect(ada.activeWorkDays).toBe(3);
    expect(ada.activeDays).toBe(4);
    expect(ada.costUsd).toBe(66);
    expect(ada.perActiveDayUsd).toBe(16.5);
    expect(ada.tools).toEqual(["codex", "claude", "copilot"]);
    expect(ben.activeWorkDays).toBe(1);
    expect(cy.habit).toBe("unseen");
  });
});

test("habits follow the share of work days", () => {
  expect(habitOf(0.75)).toBe("daily");
  expect(habitOf(0.25)).toBe("weekly");
  expect(habitOf(0.05)).toBe("sometimes");
  expect(habitOf(0)).toBe("unseen");
});

test("company totals include credits no person carries", () => {
  expect(companyTotals(ADOPTION, windowOf("2026-10-28", 1))).toEqual({
    costUsd: 72,
    tokens: 600,
    unpricedRecords: 0,
    unassignedUsd: 4,
  });
});

describe("volume bars", () => {
  test("weeks start on Monday and the current one is partial", () => {
    const bars = volumeBars(ADOPTION, "cost", "week");
    const last = bars.at(-1);
    expect(bars).toHaveLength(13);
    expect(last.start).toBe("2026-10-26");
    expect(last.end).toBe("2026-10-28");
    expect(last.partial).toBe(true);
    expect(last.tools).toEqual({ codex: 60, copilot: 5 });
    expect(bars.at(-2).tools).toEqual({ claude: 5, copilot: 2 });
  });

  test("months run from the range's first month, tokens without credits", () => {
    const bars = volumeBars(ADOPTION, "tokens", "month");
    expect(bars.map((bar) => bar.start)).toEqual(["2026-09-01", "2026-10-01"]);
    expect(bars[1].total).toBe(600);
  });

  test("the period holding today is partial through its last day", () => {
    const sunday = { ...ADOPTION, today: "2026-11-01" };
    expect(volumeBars(sunday, "cost", "week").at(-1).partial).toBe(true);
    const monthEnd = { ...ADOPTION, today: "2026-10-31" };
    expect(volumeBars(monthEnd, "cost", "month").at(-1).partial).toBe(true);
    expect(volumeBars(monthEnd, "cost", "month")[0].partial).toBe(false);
  });

  test("cost bars keep usage without a known price", () => {
    const unpriced = {
      ...ADOPTION,
      days: [{ ...day(2, "2026-10-27", "claude", 0), unpricedRecords: 3 }],
      companyDays: [],
    };
    expect(volumeBars(unpriced, "cost", "week").at(-1)).toMatchObject({
      total: 0,
      unpricedRecords: 3,
    });
    expect(volumeBars(unpriced, "tokens", "week").at(-1).unpricedRecords).toBe(
      0,
    );
  });

  test("one person's bars leave the company's credits out", () => {
    const bars = volumeBars(ADOPTION, "cost", "week", 2);
    expect(bars.at(-2).total).toBe(2);
    expect(bars.at(-1).total).toBe(0);
  });
});

test("volume steps and shares", () => {
  expect([0.5, 1, 7, 30, 500].map(volumeStep)).toEqual([0, 1, 2, 3, 4]);
  expect(topShare([1, 5, 3, 1], 2, 10)).toBe(0.8);
  expect(topShare([], 3, 0)).toBe(0);
});
