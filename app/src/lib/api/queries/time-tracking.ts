import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import dayjs from "dayjs";

export type DateRangeQuery = {
  from: string;
  to: string;
};

export type TimeEntriesQuery = DateRangeQuery & {
  unique?: boolean;
};

const timeTrackingQueryKeys = {
  projectsBase: ["time-tracking", "projects"] as const,
  activitiesBase: ["time-tracking", "activities"] as const,
  timerBase: ["time-tracking", "timer"] as const,
  timeEntriesBase: ["time-tracking", "time-entries"] as const,
  timeInfoBase: ["time-tracking", "time-info"] as const,
  timeEntryDayStatusesBase: [
    "time-tracking",
    "time-entry-day-statuses",
  ] as const,
  activities: (projectId: string) =>
    [...timeTrackingQueryKeys.activitiesBase, projectId] as const,
  timer: () => [...timeTrackingQueryKeys.timerBase, "get"] as const,
  timerHistory: () => [...timeTrackingQueryKeys.timerBase, "history"] as const,
  timeEntries: (query?: TimeEntriesQuery) =>
    [
      ...timeTrackingQueryKeys.timeEntriesBase,
      query?.from,
      query?.to,
      query?.unique,
    ] as const,
  timeInfo: (query?: DateRangeQuery) =>
    [...timeTrackingQueryKeys.timeInfoBase, query?.from, query?.to] as const,
  timeEntryDayStatuses: (query: DateRangeQuery) =>
    [
      ...timeTrackingQueryKeys.timeEntryDayStatusesBase,
      query.from,
      query.to,
    ] as const,
};

export function parseTimeEntriesQueryKey(
  queryKey: readonly unknown[],
): TimeEntriesQuery | null {
  const [scope, resource, from, to, unique] = queryKey;
  if (
    scope !== "time-tracking" ||
    resource !== "time-entries" ||
    typeof from !== "string" ||
    typeof to !== "string"
  ) {
    return null;
  }

  return {
    from,
    to,
    unique: typeof unique === "boolean" ? unique : undefined,
  };
}

export function parseTimeInfoQueryKey(
  queryKey: readonly unknown[],
): DateRangeQuery | null {
  const [scope, resource, from, to] = queryKey;
  if (
    scope !== "time-tracking" ||
    resource !== "time-info" ||
    typeof from !== "string" ||
    typeof to !== "string"
  ) {
    return null;
  }

  return { from, to };
}

export const timeTrackingQueries = {
  projectsBaseKey: timeTrackingQueryKeys.projectsBase,
  activitiesBaseKey: timeTrackingQueryKeys.activitiesBase,
  timeEntriesBaseKey: timeTrackingQueryKeys.timeEntriesBase,
  timeInfoBaseKey: timeTrackingQueryKeys.timeInfoBase,
  listProjects: () =>
    queryOptions({
      queryKey: timeTrackingQueries.projectsBaseKey,
      queryFn: async () =>
        api.get("time-tracking/projects").json<Array<Project>>(),
      staleTime: 60 * 60 * 1000,
      gcTime: 24 * 60 * 60 * 1000,
    }),
  connectionStatus: () =>
    queryOptions({
      queryKey: ["time-tracking", "connection"],
      queryFn: async () =>
        api
          .get("time-tracking/connection")
          .json<TimeTrackingConnectionStatus>(),
    }),
  listActivities: (projectId: string) =>
    queryOptions({
      queryKey: timeTrackingQueryKeys.activities(projectId),
      queryFn: async () =>
        api
          .get(`time-tracking/projects/${projectId}/activities`)
          .json<Array<Activity>>(),
      staleTime: 60 * 60 * 1000,
      gcTime: 24 * 60 * 60 * 1000,
    }),
  timerBaseKey: timeTrackingQueryKeys.timerBase,
  getTimer: () =>
    queryOptions({
      queryKey: timeTrackingQueryKeys.timer(),
      queryFn: async () =>
        api.get("time-tracking/timer").json<GetTimerResponse>(),
    }),
  timerHistory: () =>
    queryOptions({
      queryKey: timeTrackingQueryKeys.timerHistory(),
      queryFn: async () =>
        api.get("time-tracking/timer-history").json<Array<TimerHistoryEntry>>(),
    }),
  timeInfo: (query?: DateRangeQuery) =>
    queryOptions({
      queryKey: timeTrackingQueryKeys.timeInfo(query),
      queryFn: async () => {
        return api
          .get("time-tracking/time-info", {
            searchParams: query ?? {
              from: dayjs().startOf("month").format("YYYY-MM-DD"),
              to: dayjs().endOf("month").format("YYYY-MM-DD"),
            },
          })
          .json<WeeklyStats>();
      },
    }),
  timeEntries: (query?: TimeEntriesQuery) =>
    queryOptions({
      queryKey: timeTrackingQueryKeys.timeEntries(query),
      queryFn: async () => {
        return api
          .get("time-tracking/time-entries", {
            searchParams: query ?? {
              from: dayjs().startOf("month").format("YYYY-MM-DD"),
              to: dayjs().endOf("month").format("YYYY-MM-DD"),
            },
          })
          .json<Array<TimeEntry>>();
      },
      staleTime: query?.unique ? 5 * 60 * 1000 : undefined,
      gcTime: query?.unique ? 30 * 60 * 1000 : undefined,
    }),
  timeEntryDayStatusesBaseKey: timeTrackingQueryKeys.timeEntryDayStatusesBase,
  timeEntryDayStatuses: (query: DateRangeQuery) =>
    queryOptions({
      queryKey: timeTrackingQueryKeys.timeEntryDayStatuses(query),
      queryFn: async () => {
        return api
          .get("time-tracking/time-entry-day-statuses", {
            searchParams: query,
          })
          .json<Array<TimeEntryDayStatus>>();
      },
      staleTime: 2 * 60 * 1000,
      gcTime: 30 * 60 * 1000,
    }),
  adminMappings: () =>
    queryOptions({
      queryKey: ["time-tracking", "admin", "kleer-users"],
      queryFn: async () =>
        api
          .get("time-tracking/admin/kleer-users")
          .json<TimeTrackingAdminMappings>(),
    }),
};

export type GetTimerResponse = {
  timer: TimerResponse | null;
};

export type SaveTimerResponse = {
  entry: TimeEntry;
  timer: TimerResponse | null;
};

export type TimeTrackingConnectionStatus = {
  connected: boolean;
  providerUserId: string | null;
  providerUserEmail: string | null;
  providerUserName: string | null;
};

/** Active timer response. */
export type TimerResponse = {
  startTime: string;
  projectId: string | null;
  projectName: string | null;
  activityId: string | null;
  activityName: string | null;
  note: string;
  hours: number;
  minutes: number;
  seconds: number;
};

/** Timer history entry from the database. */
export type TimerHistoryEntry = {
  id: number;
  registrationId: string | null;
  userId: number;
  startTime: string;
  endTime: string | null;
  projectId: string | null;
  projectName: string | null;
  activityId: string | null;
  activityName: string | null;
  note: string | null;
  createdAt: string;
};

export type WeeklyStats = {
  workedHours: number;
  scheduledHours: number;
  remainingHours: number;
  absenceHours: number;
  coveredHours: number;
  periodFlexHours: number;
};

export type Project = {
  projectId: string;
  projectName: string;
};

export type Activity = {
  activity: string;
  activityName: string;
};

export type TimeEntry = {
  registrationId: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string; // activityId is the raw activity code used when (re)starting timers
  date: string;
  hours: number;
  note: string | null;
  startTime: string | null;
  endTime: string | null;
  weekNumber: number;
  status: TimeEntryStatus;
};

export type TimeEntryStatus = "open" | "approved" | "certified";

export type TimeEntryDayStatus = {
  date: string;
  status: TimeEntryStatus;
};

export type TimeTrackingAdminMappings = {
  users: Array<TimeTrackingAdminUser>;
  kleerUsers: Array<TimeTrackingAdminKleerUser>;
  links: Array<TimeTrackingAdminUserLink>;
};

export type TimeTrackingAdminUser = {
  id: number;
  email: string;
  fullName: string;
};

export type TimeTrackingAdminKleerUser = {
  providerUserId: string;
  foreignId: string | null;
  internalId: string | null;
  name: string;
  email: string | null;
  active: boolean;
  mappedUserId: number | null;
  mappedUserEmail: string | null;
  mappedUserName: string | null;
  lastSyncedAt: string;
};

export type TimeTrackingAdminUserLink = {
  id: number;
  userId: number;
  providerUserId: string;
  providerUserEmail: string | null;
  providerUserName: string | null;
  updatedAt: string;
};
