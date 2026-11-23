import type { QueryClient, QueryKey } from "@tanstack/react-query";
import dayjs from "dayjs";
import {
  type AbsenceEntry,
  type DateRangeQuery,
  type TimeEntry,
  type TimeEntriesQuery,
  type TimerResponse,
  parseAbsenceEntriesQueryKey,
  parseTimeEntriesQueryKey,
  parseTimeInfoQueryKey,
  timeTrackingQueries,
} from "./queries/time-tracking";
import { getWeekNumber } from "../utils";

type EntryPayload = {
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string;
  endTime: string;
  regDay?: string;
  weekNumber?: number;
  userNote: string;
};

export function buildTimeEntryFromCreatePayload(
  payload: EntryPayload,
  registrationId: string,
  syncStatus: TimeEntry["status"] = "open",
): TimeEntry {
  const start = dayjs(payload.startTime);
  const end = dayjs(payload.endTime);

  return {
    registrationId,
    projectId: payload.projectId,
    projectName: payload.projectName,
    activityId: payload.activityId,
    activityName: payload.activityName,
    date: payload.regDay ?? start.format("YYYY-MM-DD"),
    hours: end.diff(start, "hour", true),
    note: payload.userNote,
    startTime: payload.startTime,
    endTime: payload.endTime,
    weekNumber: payload.weekNumber ?? getWeekNumber(start.toDate()),
    status: syncStatus,
  };
}

export function buildTimeEntryFromSave(
  timer: TimerResponse,
  note: string | undefined,
  registrationId: string,
  syncStatus: TimeEntry["status"] = "open",
): TimeEntry | null {
  if (
    !timer.projectId ||
    !timer.projectName ||
    !timer.activityId ||
    !timer.activityName
  ) {
    return null;
  }

  return buildTimeEntryFromCreatePayload(
    {
      projectId: timer.projectId,
      projectName: timer.projectName,
      activityId: timer.activityId,
      activityName: timer.activityName,
      startTime: timer.startTime,
      endTime: new Date().toISOString(),
      userNote: note ?? timer.note,
    },
    registrationId,
    syncStatus,
  );
}

/**
 * A mutation's hold on the cached ranges of the query families it changes.
 * It changes only the ranges loaded when it began (`loaded`), whose loads it
 * cancelled so a stale answer can't overwrite its changes. A range still on
 * its first load is left to finish untouched, so a rollback never takes back
 * what was never added. Once the last pending mutation settles, every range
 * of the families they changed reloads from the server.
 */
export type OptimisticRanges = {
  loaded: ReadonlySet<string>;
  families: ReadonlyArray<QueryKey>;
};

export const NO_RANGES: OptimisticRanges = { loaded: new Set(), families: [] };

export function upsertEntryInCachedRanges(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  entry: TimeEntry,
) {
  for (const { params, queryKey } of entryRanges(queryClient, ranges)) {
    if (!isEntryInRange(params, entry)) continue;

    queryClient.setQueryData(queryKey, (current) => {
      if (!current) return current;
      const withoutEntry = current.filter(
        (item) => item.registrationId !== entry.registrationId,
      );
      const next = [entry, ...withoutEntry].sort(compareEntries);
      return params.unique ? dedupeUniqueEntries(next) : next;
    });
  }
}

export function replaceEntryInCachedRanges(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  oldId: string,
  entry: TimeEntry,
) {
  for (const { params, queryKey } of entryRanges(queryClient, ranges)) {
    queryClient.setQueryData(queryKey, (current) => {
      if (!current) return current;
      const withoutOld = current.filter(
        (item) => item.registrationId !== oldId,
      );
      const withoutNew = withoutOld.filter(
        (item) => item.registrationId !== entry.registrationId,
      );
      const next = isEntryInRange(params, entry)
        ? [entry, ...withoutNew].sort(compareEntries)
        : withoutNew;
      return params.unique ? dedupeUniqueEntries(next) : next;
    });
  }
}

export function removeEntryFromCachedRanges(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  registrationId: string,
) {
  for (const { queryKey } of entryRanges(queryClient, ranges)) {
    queryClient.setQueryData(queryKey, (current) =>
      current?.filter((entry) => entry.registrationId !== registrationId),
    );
  }
}

export function upsertAbsenceInCachedRanges(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  entry: AbsenceEntry,
) {
  for (const { params, queryKey } of absenceRanges(queryClient, ranges)) {
    if (!isDateInRange(params, entry.date)) continue;

    queryClient.setQueryData(queryKey, (current) => {
      if (!current) return current;
      const withoutEntry = current.filter(
        (item) => item.absenceId !== entry.absenceId,
      );
      return [entry, ...withoutEntry].sort(compareAbsences);
    });
  }
}

export function replaceAbsencesInCachedRanges(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  oldIds: Array<string>,
  entries: Array<AbsenceEntry>,
) {
  const idsToReplace = new Set([
    ...oldIds,
    ...entries.map((entry) => entry.absenceId),
  ]);

  for (const { params, queryKey } of absenceRanges(queryClient, ranges)) {
    queryClient.setQueryData(queryKey, (current) => {
      if (!current) return current;
      const withoutOld = current.filter(
        (item) => !idsToReplace.has(item.absenceId),
      );
      return [
        ...entries.filter((entry) => isDateInRange(params, entry.date)),
        ...withoutOld,
      ].sort(compareAbsences);
    });
  }
}

export function removeAbsenceFromCachedRanges(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  absenceId: string,
) {
  for (const { queryKey } of absenceRanges(queryClient, ranges)) {
    queryClient.setQueryData(queryKey, (current) =>
      current?.filter((entry) => entry.absenceId !== absenceId),
    );
  }
}

export function applyTimeInfoDelta(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  date: string,
  deltaHours: number,
) {
  for (const { params, queryKey } of timeInfoRanges(queryClient, ranges)) {
    if (!isDateInRange(params, date)) continue;

    queryClient.setQueryData(queryKey, (current) => {
      if (!current) return current;

      return {
        ...current,
        workedHours: current.workedHours + deltaHours,
        coveredHours: current.coveredHours + deltaHours,
        remainingHours: current.remainingHours - deltaHours,
        periodFlexHours: current.periodFlexHours + deltaHours,
      };
    });
  }
}

export function applyAbsenceTimeInfoDelta(
  queryClient: QueryClient,
  ranges: OptimisticRanges,
  date: string,
  deltaHours: number,
) {
  for (const { params, queryKey } of timeInfoRanges(queryClient, ranges)) {
    if (!isDateInRange(params, date)) continue;

    queryClient.setQueryData(queryKey, (current) => {
      if (!current) return current;

      return {
        ...current,
        absenceHours: current.absenceHours + deltaHours,
        coveredHours: current.coveredHours + deltaHours,
        remainingHours: current.remainingHours - deltaHours,
        periodFlexHours: current.periodFlexHours + deltaHours,
      };
    });
  }
}

/** The flex balance has no optimistic updates to keep, so it reloads at once;
 * the time entry ranges reload when their mutations settle. */
export function invalidateFlexBalance(queryClient: QueryClient) {
  reloadQueries(queryClient, timeTrackingQueries.flexBalanceBaseKey);
}

/** Reloads queries the server just changed. A first load still under way
 * would be reused by the refetch and may predate the change, so it is
 * cancelled for a new request. */
export function reloadQueries(queryClient: QueryClient, queryKey: QueryKey) {
  void queryClient
    .cancelQueries({ queryKey })
    .then(() => queryClient.invalidateQueries({ queryKey }));
}

export function cancelTimeTrackingRangeQueries(queryClient: QueryClient) {
  return beginRangeMutation(queryClient, [
    timeTrackingQueries.timeEntriesBaseKey,
    timeTrackingQueries.timeInfoBaseKey,
  ]);
}

export function cancelAbsenceQueries(queryClient: QueryClient) {
  return beginRangeMutation(queryClient, [
    timeTrackingQueries.absenceEntriesBaseKey,
    timeTrackingQueries.timeInfoBaseKey,
  ]);
}

type RangeMutations = {
  pending: Set<OptimisticRanges>;
  /** Families to reload once no range mutation is pending. */
  toReload: Map<string, QueryKey>;
};

const rangeMutations = new WeakMap<QueryClient, RangeMutations>();

function rangeMutationsOf(queryClient: QueryClient) {
  let state = rangeMutations.get(queryClient);
  if (!state) {
    state = { pending: new Set(), toReload: new Map() };
    rangeMutations.set(queryClient, state);
  }
  return state;
}

async function beginRangeMutation(
  queryClient: QueryClient,
  families: Array<QueryKey>,
): Promise<OptimisticRanges> {
  const loaded = families.flatMap((queryKey) =>
    queryClient.getQueryCache().findAll({
      queryKey,
      predicate: (query) => query.state.data !== undefined,
    }),
  );
  const ranges = {
    loaded: new Set(loaded.map((query) => query.queryHash)),
    families,
  };
  // Pending before the await, so no reload starts meanwhile.
  rangeMutationsOf(queryClient).pending.add(ranges);
  await Promise.all(
    loaded.map((query) =>
      queryClient.cancelQueries({ queryKey: query.queryKey, exact: true }),
    ),
  );
  return ranges;
}

/**
 * Ends a mutation's hold. Once no range mutation is pending, every range of
 * the families the settled mutations changed reloads from the server, so the
 * cache ends up matching it however their answers and requests interleaved.
 * A reload waits while any mutation is pending, since it would replace the
 * optimistic changes that mutation's rollback expects to find.
 */
export function settleRangeMutation(
  queryClient: QueryClient,
  ranges: OptimisticRanges | undefined,
) {
  const state = rangeMutationsOf(queryClient);
  if (ranges) {
    state.pending.delete(ranges);
    for (const family of ranges.families) {
      state.toReload.set(JSON.stringify(family), family);
    }
  }
  if (state.pending.size > 0 || state.toReload.size === 0) return;

  const families = [...state.toReload.values()];
  // A first load still under way would be reused by a refetch, so every
  // request is cancelled for one sent after the mutations.
  void Promise.all(
    families.map((queryKey) => queryClient.cancelQueries({ queryKey })),
  ).then(() => {
    // A mutation that began meanwhile reloads them when it settles.
    if (state.pending.size > 0) return;
    state.toReload.clear();
    for (const queryKey of families) {
      queryClient.invalidateQueries({ queryKey });
    }
  });
}

export function findCachedEntry(
  queryClient: QueryClient,
  registrationId: string,
): TimeEntry | undefined {
  for (const { entries } of getTimeEntryCaches(queryClient)) {
    const found = entries?.find(
      (entry) => entry.registrationId === registrationId,
    );
    if (found) return found;
  }
}

export function findCachedAbsence(
  queryClient: QueryClient,
  absenceId: string,
): AbsenceEntry | undefined {
  for (const { absences } of getAbsenceEntryCaches(queryClient)) {
    const found = absences?.find((entry) => entry.absenceId === absenceId);
    if (found) return found;
  }
}

export function getCachedTimeEntries(
  queryClient: QueryClient,
): Array<TimeEntry> {
  return getTimeEntryCaches(queryClient).flatMap(
    ({ entries }) => entries ?? [],
  );
}

export function setTimerCache(
  queryClient: QueryClient,
  timer: TimerResponse | null,
) {
  const query = timeTrackingQueries.getTimer();
  queryClient.setQueryData(query.queryKey, { timer });
}

function getTimeEntryCaches(queryClient: QueryClient) {
  return queryClient
    .getQueryCache()
    .findAll({ queryKey: timeTrackingQueries.timeEntriesBaseKey })
    .flatMap((query) => {
      const params = parseTimeEntriesQueryKey(query.queryKey);
      if (!params) return [];

      const options = timeTrackingQueries.timeEntries(params);
      return [
        {
          params,
          entries: queryClient.getQueryData(options.queryKey),
        },
      ];
    });
}

function getAbsenceEntryCaches(queryClient: QueryClient) {
  return queryClient
    .getQueryCache()
    .findAll({ queryKey: timeTrackingQueries.absenceEntriesBaseKey })
    .flatMap((query) => {
      const params = parseAbsenceEntriesQueryKey(query.queryKey);
      if (!params) return [];

      const options = timeTrackingQueries.absenceEntries(params);
      return [
        {
          params,
          absences: queryClient.getQueryData(options.queryKey),
        },
      ];
    });
}

function optimisticQueries(
  queryClient: QueryClient,
  queryKey: QueryKey,
  ranges: OptimisticRanges,
) {
  return queryClient.getQueryCache().findAll({
    queryKey,
    predicate: (query) => ranges.loaded.has(query.queryHash),
  });
}

function entryRanges(queryClient: QueryClient, ranges: OptimisticRanges) {
  return optimisticQueries(
    queryClient,
    timeTrackingQueries.timeEntriesBaseKey,
    ranges,
  ).flatMap((query) => {
    const params = parseTimeEntriesQueryKey(query.queryKey);
    return params
      ? [{ params, queryKey: timeTrackingQueries.timeEntries(params).queryKey }]
      : [];
  });
}

function absenceRanges(queryClient: QueryClient, ranges: OptimisticRanges) {
  return optimisticQueries(
    queryClient,
    timeTrackingQueries.absenceEntriesBaseKey,
    ranges,
  ).flatMap((query) => {
    const params = parseAbsenceEntriesQueryKey(query.queryKey);
    return params
      ? [
          {
            params,
            queryKey: timeTrackingQueries.absenceEntries(params).queryKey,
          },
        ]
      : [];
  });
}

function timeInfoRanges(queryClient: QueryClient, ranges: OptimisticRanges) {
  return optimisticQueries(
    queryClient,
    timeTrackingQueries.timeInfoBaseKey,
    ranges,
  ).flatMap((query) => {
    const params = parseTimeInfoQueryKey(query.queryKey);
    return params
      ? [{ params, queryKey: timeTrackingQueries.timeInfo(params).queryKey }]
      : [];
  });
}

function isEntryInRange(params: TimeEntriesQuery, entry: TimeEntry) {
  return isDateInRange(params, entry.date);
}

function isDateInRange({ from, to }: DateRangeQuery, date: string) {
  return from <= date && date <= to;
}

function dedupeUniqueEntries(entries: Array<TimeEntry>) {
  const seen = new Set<string>();
  return entries.filter((entry) => {
    const key = `${entry.projectName}\u0000${entry.activityName}\u0000${entry.note ?? ""}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function compareEntries(a: TimeEntry, b: TimeEntry) {
  const dateCompare = b.date.localeCompare(a.date);
  if (dateCompare !== 0) return dateCompare;
  return (b.startTime ?? "").localeCompare(a.startTime ?? "");
}

function compareAbsences(a: AbsenceEntry, b: AbsenceEntry) {
  const dateCompare = b.date.localeCompare(a.date);
  if (dateCompare !== 0) return dateCompare;
  return a.absenceId.localeCompare(b.absenceId);
}
