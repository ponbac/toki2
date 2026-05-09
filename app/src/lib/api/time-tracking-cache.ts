import type { QueryClient } from "@tanstack/react-query";
import dayjs from "dayjs";
import {
  type DateRangeQuery,
  type TimeEntry,
  type TimeEntriesQuery,
  type TimerResponse,
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

export function upsertEntryInCachedRanges(
  queryClient: QueryClient,
  entry: TimeEntry,
) {
  for (const { params } of getTimeEntryCaches(queryClient)) {
    if (!isEntryInRange(params, entry)) continue;

    const query = timeTrackingQueries.timeEntries(params);
    queryClient.setQueryData(query.queryKey, (current = []) => {
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
  oldId: string,
  entry: TimeEntry,
) {
  for (const { params } of getTimeEntryCaches(queryClient)) {
    const query = timeTrackingQueries.timeEntries(params);
    queryClient.setQueryData(query.queryKey, (current = []) => {
      const withoutOld = current.filter((item) => item.registrationId !== oldId);
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
  registrationId: string,
) {
  for (const { params } of getTimeEntryCaches(queryClient)) {
    const query = timeTrackingQueries.timeEntries(params);
    queryClient.setQueryData(query.queryKey, (current = []) =>
      current.filter((entry) => entry.registrationId !== registrationId),
    );
  }
}

export function applyTimeInfoDelta(
  queryClient: QueryClient,
  date: string,
  deltaHours: number,
) {
  for (const params of getTimeInfoCacheParams(queryClient)) {
    if (!isDateInRange(params, date)) continue;

    const query = timeTrackingQueries.timeInfo(params);
    queryClient.setQueryData(query.queryKey, (current) => {
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

export function markTimeTrackingListsStale(queryClient: QueryClient) {
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.timeEntriesBaseKey,
    refetchType: "none",
  });
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.timeInfoBaseKey,
    refetchType: "none",
  });
}

export async function cancelTimeTrackingRangeQueries(queryClient: QueryClient) {
  await Promise.all([
    queryClient.cancelQueries({
      queryKey: timeTrackingQueries.timeEntriesBaseKey,
    }),
    queryClient.cancelQueries({
      queryKey: timeTrackingQueries.timeInfoBaseKey,
    }),
  ]);
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

export function getCachedTimeEntries(
  queryClient: QueryClient,
): Array<TimeEntry> {
  return getTimeEntryCaches(queryClient).flatMap(({ entries }) => entries ?? []);
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

function getTimeInfoCacheParams(queryClient: QueryClient): Array<DateRangeQuery> {
  return queryClient
    .getQueryCache()
    .findAll({ queryKey: timeTrackingQueries.timeInfoBaseKey })
    .flatMap((query) => {
      const params = parseTimeInfoQueryKey(query.queryKey);
      return params ? [params] : [];
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
