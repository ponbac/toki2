import { describe, expect, test } from "bun:test";
import { flexBreakdown, loadFlexBalanceBeforeToday } from "./flex-breakdown.ts";

const TODAY = "2026-11-23";

function balanceThrough(through, { settledThrough = "2026-10-31" } = {}) {
  const settledHours = 2;
  const estimatedHours = settledThrough < through ? 3.5 : 0;
  return {
    balanceHours: settledHours + estimatedHours,
    settledHours,
    settledThrough,
    estimatedHours,
    through,
  };
}

function dayStats({ scheduledHours = 8, coveredHours = 0 } = {}) {
  return {
    workedHours: coveredHours,
    scheduledHours,
    remainingHours: scheduledHours - coveredHours,
    absenceHours: 0,
    coveredHours,
    periodFlexHours: coveredHours - scheduledHours,
  };
}

describe("loadFlexBalanceBeforeToday", () => {
  test("asks for the balance through yesterday", async () => {
    const asked = [];
    const loaded = await loadFlexBalanceBeforeToday(TODAY, async (through) => {
      asked.push(through);
      return balanceThrough(through);
    });

    expect(asked).toEqual(["2026-11-22"]);
    expect(loaded).toEqual({
      balance: balanceThrough("2026-11-22"),
      includesToday: false,
    });
  });

  test("falls back to the balance through today once today is settled", async () => {
    const asked = [];
    const loaded = await loadFlexBalanceBeforeToday(TODAY, async (through) => {
      asked.push(through);
      return through < TODAY
        ? "before-settlement"
        : balanceThrough(through, { settledThrough: TODAY });
    });

    expect(asked).toEqual(["2026-11-22", TODAY]);
    expect(loaded?.includesToday).toBe(true);
  });

  test("shows nothing when the provider has settled past today", async () => {
    const loaded = await loadFlexBalanceBeforeToday(
      TODAY,
      async () => "before-settlement",
    );

    expect(loaded).toBeNull();
  });

  test("shows nothing, without a second request, when no balance is kept", async () => {
    const asked = [];
    const loaded = await loadFlexBalanceBeforeToday(TODAY, async (through) => {
      asked.push(through);
      return "none";
    });

    expect(asked).toEqual(["2026-11-22"]);
    expect(loaded).toBeNull();
  });
});

describe("flexBreakdown", () => {
  const beforeToday = {
    balance: balanceThrough("2026-11-22"),
    includesToday: false,
  };

  test("a workday splits the balance before today from today's hours", () => {
    const breakdown = flexBreakdown(
      beforeToday,
      dayStats({ coveredHours: 1.5 }),
      0,
    );

    expect(breakdown).toEqual({
      settled: { hours: 2, through: "2026-10-31" },
      earlier: { from: "2026-11-01", through: "2026-11-22", hours: 3.5 },
      beforeToday: 5.5,
      today: { hours: -6.5, coveredHours: 1.5, scheduledHours: 8 },
      timerHours: null,
      nowHours: -1,
    });
  });

  test("a running timer counts toward today and the balance now, not before today", () => {
    const breakdown = flexBreakdown(
      beforeToday,
      dayStats({ coveredHours: 1.5 }),
      0.5,
    );

    expect(breakdown.beforeToday).toBe(5.5);
    expect(breakdown.today).toEqual({
      hours: -6,
      coveredHours: 2,
      scheduledHours: 8,
    });
    expect(breakdown.nowHours).toBe(-0.5);
  });

  test("the first day after the settled month has no estimate before it", () => {
    const breakdown = flexBreakdown(
      {
        balance: balanceThrough("2026-10-31"),
        includesToday: false,
      },
      dayStats(),
      0,
    );

    expect(breakdown.earlier).toBeNull();
    expect(breakdown.beforeToday).toBe(2);
    expect(breakdown.nowHours).toBe(-6);
  });

  test("a day with nothing scheduled or booked shows no today", () => {
    const breakdown = flexBreakdown(
      beforeToday,
      dayStats({ scheduledHours: 0 }),
      0,
    );

    expect(breakdown.today).toBeNull();
    expect(breakdown.nowHours).toBe(5.5);
  });

  test("hours booked on a day with nothing scheduled count as today", () => {
    const breakdown = flexBreakdown(
      beforeToday,
      dayStats({ scheduledHours: 0, coveredHours: 1 }),
      0,
    );

    expect(breakdown.today).toEqual({
      hours: 1,
      coveredHours: 1,
      scheduledHours: 0,
    });
    expect(breakdown.nowHours).toBe(6.5);
  });

  test("the balance before today shows while today's hours load", () => {
    const breakdown = flexBreakdown(beforeToday, undefined, 0.5);

    expect(breakdown.beforeToday).toBe(5.5);
    expect(breakdown.today).toBeNull();
    expect(breakdown.nowHours).toBeNull();
  });

  test("a settled today shows the running timer on its own row", () => {
    const breakdown = flexBreakdown(
      {
        balance: balanceThrough(TODAY, { settledThrough: TODAY }),
        includesToday: true,
      },
      dayStats({ coveredHours: 8 }),
      0.5,
    );

    expect(breakdown.earlier).toBeNull();
    expect(breakdown.beforeToday).toBeNull();
    expect(breakdown.today).toBeNull();
    expect(breakdown.timerHours).toBe(0.5);
    expect(breakdown.nowHours).toBe(2.5);
  });
});
