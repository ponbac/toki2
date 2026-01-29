import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import dayjs from "dayjs";

export const timeTrackingQueries = {
  listProjects: () =>
    queryOptions({
      queryKey: ["time-tracking", "projects"],
      queryFn: async () =>
        api.get("time-tracking/projects").json<Array<Project>>(),
    }),
  listActivities: (projectId: string) =>
    queryOptions({
      queryKey: ["time-tracking", "activities", projectId],
      queryFn: async () =>
        api
          .get(`time-tracking/projects/${projectId}/activities`)
          .json<Array<Activity>>(),
    }),
  timerBaseKey: ["time-tracking", "timer"],
  getTimer: () =>
    queryOptions({
      queryKey: [...timeTrackingQueries.timerBaseKey, "get"],
      queryFn: async () =>
        api.get("time-tracking/timer").json<GetTimerResponse>(),
    }),
  timerHistory: () =>
    queryOptions({
      queryKey: [...timeTrackingQueries.timerBaseKey, "history"],
      queryFn: async () =>
        api.get("time-tracking/timer-history").json<Array<TimerHistoryEntry>>(),
    }),
  timeInfo: (query?: { from: string; to: string }) =>
    queryOptions({
      queryKey: ["time-tracking", "time-info", query?.from, query?.to],
      queryFn: async () => {
        return api
          .get("time-tracking/time-info", {
            searchParams: query ?? {
              from: dayjs().startOf("month").format("YYYY-MM-DD"),
              to: dayjs().endOf("month").format("YYYY-MM-DD"),
            },
          })
          .json<TimeInfo>();
      },
    }),
  timeEntries: (query?: { from: string; to: string; unique?: boolean }) =>
    queryOptions({
      queryKey: [
        "time-tracking",
        "time-entries",
        query?.from,
        query?.to,
        query?.unique,
      ],
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
    }),
};

export type GetTimerResponse = {
  timer: TimerResponse | null;
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

export type TimeInfo = {
  periodTimeLeft: number;
  workedPeriodTime: number;
  scheduledPeriodTime: number;
  workedPeriodWithAbsenceTime: number;
  flexTimeCurrent: number;
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
  attestLevel: AttestLevel;
};

export enum AttestLevel {
  None = 0,
  Week = 1,
  Month = 2,
}
