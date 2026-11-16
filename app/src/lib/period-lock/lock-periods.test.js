import { describe, expect, test } from "bun:test";
import {
  eachDate,
  findDueLock,
  hoursOver,
  hoursShort,
  listCheckpoints,
  lockLevel,
  rangesWithStatus,
  weekParts,
  workweekRange,
} from "./lock-periods.ts";
import { formatHoursMinutes } from "../utils.ts";

function workdays(from, to, today, lockedThrough) {
  return eachDate({ from, to }).map((date) => {
    const weekday = new Date(`${date}T12:00:00Z`).getUTCDay();
    const scheduledHours = weekday === 0 || weekday === 6 ? 0 : 8;
    return {
      date,
      status: date <= lockedThrough ? "approved" : "open",
      reportedHours: date <= today ? scheduledHours : 0,
      scheduledHours,
      entries: [],
      hasOverlap: false,
      isFuture: date > today,
      isToday: date === today,
      isIdle: scheduledHours === 0,
    };
  });
}

function dueOn(today, lockedThrough, from = "2026-09-01", to = "2026-11-01") {
  const days = workdays(from, to, today, lockedThrough);
  return findDueLock(days, listCheckpoints(days, today), today);
}

describe("what has to be locked", () => {
  test("nothing before the last scheduled day of the week", () => {
    expect(dueOn("2026-10-27", "2026-10-25")).toBeNull();
  });

  test("a week and a month ending together are one lock", () => {
    const due = dueOn("2026-10-30", "2026-10-25");
    expect(due?.title).toBe("Week 44 and October are due today");
    expect([due?.from, due?.to]).toEqual(["2026-10-26", "2026-11-01"]);
  });

  test("a month ending midweek locks only through its last day", () => {
    const due = dueOn("2026-09-30", "2026-09-27", "2026-08-01", "2026-10-04");
    expect(due?.title).toBe("September is due today");
    expect(due?.to).toBe("2026-09-30");
  });

  test("an empty weekend left open keeps the week and month due", () => {
    const due = dueOn("2026-10-30", "2026-10-30");
    expect(due?.title).toBe("Week 44 and October are due today");
    expect([due?.from, due?.to]).toEqual(["2026-10-31", "2026-11-01"]);
  });

  test("an unlocked week is overdue once it has ended", () => {
    const due = dueOn("2026-10-27", "2026-10-18");
    expect(due?.title).toBe("Week 43 is overdue");
    expect(due?.severity).toBe("overdue");
  });

  test("an unlocked month is overdue from the 1st", () => {
    const due = dueOn("2026-10-01", "2026-09-27", "2026-08-01", "2026-10-04");
    expect(due?.title).toBe("September is overdue");
    expect(due?.severity).toBe("month-overdue");
  });

  test("the last scheduled day moves the deadline, not the weekday", () => {
    const days = workdays("2026-10-26", "2026-11-01", "2026-10-29", "2026-10-25");
    const holiday = days.find((day) => day.date === "2026-10-30");
    holiday.scheduledHours = 0;
    holiday.isIdle = true;

    const week = listCheckpoints(days, "2026-10-29").find(
      (checkpoint) => checkpoint.kind === "week",
    );
    expect(week).toMatchObject({ dueOn: "2026-10-29", state: "due" });
  });
});

describe("how loudly to ask", () => {
  test("stays silent until the day before a deadline", () => {
    expect(lockLevel(null, "2026-10-30", "2026-10-27")).toBe("clear");
    expect(lockLevel(null, "2026-10-30", "2026-10-29")).toBe("soon");
  });

  test("today's unfinished hours are no reason to raise a flag", () => {
    const due = dueOn("2026-10-30", "2026-10-25");
    expect(due?.detail).toBe("26 Oct – 1 Nov.");
  });
});

test("unlocking skips certified days instead of spanning them", () => {
  const days = workdays("2026-10-19", "2026-10-23", "2026-10-23", "2026-10-23");
  days[2].status = "certified";

  expect(rangesWithStatus(days, "approved")).toEqual([
    { from: "2026-10-19", to: "2026-10-20" },
    { from: "2026-10-22", to: "2026-10-23" },
  ]);
});

describe("locking Monday to Friday", () => {
  test("a Sunday belongs to the week that started six days before", () => {
    expect(workweekRange("2026-11-08")).toEqual({
      from: "2026-11-02",
      to: "2026-11-06",
    });
  });

  test("a week crossing a month end keeps its Friday", () => {
    expect(workweekRange("2026-07-01")).toEqual({
      from: "2026-06-29",
      to: "2026-07-03",
    });
  });

  test("a month ending on a Friday offers Mon–Fri once", () => {
    expect(weekParts("2026-07-29")).toEqual([
      { from: "2026-07-27", to: "2026-07-31" },
      { from: "2026-08-01", to: "2026-08-02" },
    ]);
  });
});

describe("how far each day is from its schedule", () => {
  // Three short days, then two long ones.
  const reported = [6.65, 5.64, 7.2, 9.27, 8.74];
  const week = workdays("2026-11-09", "2026-11-15", "2026-11-16", "").map(
    (day, index) => ({ ...day, reportedHours: reported[index] ?? 0 }),
  );
  const shortAndOver = (day) => [
    formatHoursMinutes(hoursShort(day)),
    formatHoursMinutes(hoursOver(day)),
  ];
  const finishedDay = (reportedHours, changes = {}) => ({
    ...workdays("2026-11-12", "2026-11-12", "2026-11-16", "")[0],
    reportedHours,
    ...changes,
  });

  test("long days have a surplus and short days a shortfall", () => {
    expect(week.map(shortAndOver)).toEqual([
      ["01:21", "00:00"],
      ["02:22", "00:00"],
      ["00:48", "00:00"],
      ["00:00", "01:16"],
      ["00:00", "00:44"],
      ["00:00", "00:00"],
      ["00:00", "00:00"],
    ]);
  });

  test("a day on schedule has no surplus", () => {
    expect(hoursOver(finishedDay(8))).toBe(0);
    expect(hoursOver(finishedDay(8 + 0.4 / 60))).toBe(0);
  });

  test("a surplus counts from the first whole minute", () => {
    expect(formatHoursMinutes(hoursOver(finishedDay(8 + 0.6 / 60)))).toBe(
      "00:01",
    );
    expect(formatHoursMinutes(hoursOver(finishedDay(8 + 5 / 60)))).toBe(
      "00:05",
    );
  });

  test("weekend work is all surplus", () => {
    const saturday = { ...week[5], reportedHours: 2.5, isIdle: false };
    expect(formatHoursMinutes(hoursOver(saturday))).toBe("02:30");
  });

  test("today, future and locked days have no surplus", () => {
    expect(hoursOver(finishedDay(10, { isToday: true }))).toBe(0);
    expect(hoursOver(finishedDay(10, { isFuture: true }))).toBe(0);
    expect(hoursOver(finishedDay(10, { status: "approved" }))).toBe(0);
  });
});
