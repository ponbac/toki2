import { describe, expect, mock, test } from "bun:test";
import { QueryClient, QueryObserver } from "@tanstack/react-query";

// The cache builds query keys from the query factories, which import the
// browser-only HTTP client; nothing here sends a request.
mock.module("./api.ts", () => ({ api: {}, API_URL: "" }));

const {
  applyTimeInfoDelta,
  cancelAbsenceQueries,
  cancelTimeTrackingRangeQueries,
  invalidateFlexBalance,
  removeEntryFromCachedRanges,
  settleRangeMutation,
  upsertEntryInCachedRanges,
} = await import("./time-tracking-cache.ts");
const { timeTrackingQueries } = await import("./queries/time-tracking.ts");

const TODAY = "2026-11-23";
const todayKey = timeTrackingQueries.timeInfo({
  from: TODAY,
  to: TODAY,
}).queryKey;
const entriesKey = timeTrackingQueries.timeEntries({
  from: TODAY,
  to: TODAY,
}).queryKey;

function dayStats(coveredHours) {
  return {
    workedHours: coveredHours,
    scheduledHours: 8,
    remainingHours: 8 - coveredHours,
    absenceHours: 0,
    coveredHours,
    periodFlexHours: coveredHours - 8,
  };
}

function entry(registrationId, hours) {
  return {
    registrationId,
    projectId: "p1",
    projectName: "Widget API",
    activityId: "a1",
    activityName: "Development",
    date: TODAY,
    hours,
    note: null,
    startTime: null,
    endTime: null,
    weekNumber: 48,
    status: "open",
  };
}

/** A shown query whose answer arrives when `answer` is called. */
function watch(queryClient, queryKey) {
  let calls = 0;
  const pending = [];
  const observer = new QueryObserver(queryClient, {
    queryKey,
    queryFn: () => {
      calls += 1;
      return new Promise((resolve) => pending.push(resolve));
    },
  });
  observer.subscribe(() => {});
  return {
    calls: () => calls,
    /** Answers the `nth` request still waiting, the oldest by default. */
    answer: async (data, nth = 0) => {
      pending.splice(nth, 1)[0](data);
      await settled();
    },
  };
}

const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("a mutation that starts while a range first loads", () => {
  test("a failed save's rollback leaves the range's answer intact", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);

    const save = await cancelTimeTrackingRangeQueries(queryClient);
    applyTimeInfoDelta(queryClient, save, TODAY, 2);
    await today.answer(dayStats(1));
    applyTimeInfoDelta(queryClient, save, TODAY, -2);

    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(1));
  });

  test("the range reloads once the mutation settles", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);

    const save = await cancelTimeTrackingRangeQueries(queryClient);
    await today.answer(dayStats(1));
    settleRangeMutation(queryClient, save);
    await settled();
    await today.answer(dayStats(3));

    expect(today.calls()).toBe(2);
    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(3));
  });

  test("a first load still under way when it settles is asked again", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);

    const save = await cancelTimeTrackingRangeQueries(queryClient);
    settleRangeMutation(queryClient, save);
    await settled();
    // The new request answers first; the first load, from before the save,
    // arrives last.
    await today.answer(dayStats(3), 1);
    await today.answer(dayStats(1), 0);

    expect(today.calls()).toBe(2);
    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(3));
  });

  test("an entry list keeps the server's entries, not just the new one", async () => {
    const queryClient = new QueryClient();
    const entries = watch(queryClient, entriesKey);

    const first = await cancelTimeTrackingRangeQueries(queryClient);
    upsertEntryInCachedRanges(queryClient, first, entry("new", 1));
    // A second mutation before the first load answers.
    await cancelTimeTrackingRangeQueries(queryClient);
    await entries.answer([entry("existing", 2)]);

    expect(queryClient.getQueryData(entriesKey)).toEqual([
      entry("existing", 2),
    ]);
  });
});

describe("overlapping mutations", () => {
  test("a reload waits for a later mutation's rollback", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);

    const first = await cancelTimeTrackingRangeQueries(queryClient);
    await today.answer(dayStats(2));
    const second = await cancelTimeTrackingRangeQueries(queryClient);
    applyTimeInfoDelta(queryClient, second, TODAY, 2);
    settleRangeMutation(queryClient, first);
    await settled();

    expect(today.calls()).toBe(1);

    // The second mutation fails and rolls back.
    applyTimeInfoDelta(queryClient, second, TODAY, -2);

    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(2));

    settleRangeMutation(queryClient, second);
    await settled();
    await today.answer(dayStats(2));

    expect(today.calls()).toBe(2);
    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(2));
  });

  test("an absence settling doesn't bring back an entry being deleted", async () => {
    const queryClient = new QueryClient();
    const entries = watch(queryClient, entriesKey);
    await entries.answer([entry("doomed", 1)]);

    const deletion = await cancelTimeTrackingRangeQueries(queryClient);
    removeEntryFromCachedRanges(queryClient, deletion, "doomed");
    const absence = await cancelAbsenceQueries(queryClient);
    settleRangeMutation(queryClient, absence);
    await settled();

    expect(entries.calls()).toBe(1);
    expect(queryClient.getQueryData(entriesKey)).toEqual([]);
  });
});

describe("a mutation on a loaded range", () => {
  test("its optimistic hours survive a reload that was under way", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);
    await today.answer(dayStats(1));
    queryClient.refetchQueries({ queryKey: todayKey });

    const save = await cancelTimeTrackingRangeQueries(queryClient);
    applyTimeInfoDelta(queryClient, save, TODAY, 2);
    // The reload's answer, from before the save, arrives after it.
    await today.answer(dayStats(1));

    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(3));
  });

  test("a second save during the first one's reload still ends on the server's hours", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);
    await today.answer(dayStats(1));

    const first = await cancelTimeTrackingRangeQueries(queryClient);
    applyTimeInfoDelta(queryClient, first, TODAY, 2);
    settleRangeMutation(queryClient, first);
    await settled();
    const second = await cancelTimeTrackingRangeQueries(queryClient);
    applyTimeInfoDelta(queryClient, second, TODAY, 1);
    // The first save's reload, cancelled by the second, answers anyway.
    await today.answer(dayStats(3));
    settleRangeMutation(queryClient, second);
    await settled();
    await today.answer(dayStats(4));

    expect(today.calls()).toBe(3);
    expect(queryClient.getQueryData(todayKey)).toEqual(dayStats(4));
  });

  test("a mutation that begins as another settles holds back the reload", async () => {
    const queryClient = new QueryClient();
    const today = watch(queryClient, todayKey);
    await today.answer(dayStats(1));

    const first = await cancelTimeTrackingRangeQueries(queryClient);
    settleRangeMutation(queryClient, first);
    const second = cancelTimeTrackingRangeQueries(queryClient);
    await settled();

    expect(today.calls()).toBe(1);

    settleRangeMutation(queryClient, await second);
    await settled();

    expect(today.calls()).toBe(2);
  });
});

describe("the flex balance after a change", () => {
  test("a first load still under way is asked again", async () => {
    const queryClient = new QueryClient();
    const flexKey = timeTrackingQueries.flexBalanceBeforeToday(TODAY).queryKey;
    const flex = watch(queryClient, flexKey);

    invalidateFlexBalance(queryClient);
    await settled();
    // The new request answers first; the first load, from before the
    // change, arrives last.
    await flex.answer({ hours: 4 }, 1);
    await flex.answer({ hours: 2 }, 0);

    expect(flex.calls()).toBe(2);
    expect(queryClient.getQueryData(flexKey)).toEqual({ hours: 4 });
  });
});
