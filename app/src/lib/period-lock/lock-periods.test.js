import { describe, expect, test } from "bun:test";
import {
  eachDate,
  findDueLock,
  listCheckpoints,
  lockLevel,
  rangesWithStatus,
} from "./lock-periods.ts";

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
