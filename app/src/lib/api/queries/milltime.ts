import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import dayjs from "dayjs";

export const milltimeQueries = {
  listProjects: () =>
    queryOptions({
      queryKey: ["milltime", "projects"],
      queryFn: async () =>
        api.get("milltime/projects").json<Array<Project>>(),
    }),
  listActivities: (projectId: string) =>
    queryOptions({
      queryKey: ["milltime", "activities", projectId],
      queryFn: async () =>
        api
          .get(`milltime/projects/${projectId}/activities`)
          .json<Array<Activity>>(),
    }),
  timerBaseKey: ["milltime", "timer"],
  getTimer: () =>
    queryOptions({
      queryKey: [...milltimeQueries.timerBaseKey, "get"],
      queryFn: async () =>
        api.get("milltime/timer").json<GetTimerResponse>(),
    }),
  timerHistory: () =>
    queryOptions({
      queryKey: [...milltimeQueries.timerBaseKey, "history"],
      queryFn: async () =>
        api.get("milltime/timer-history").json<Array<TimerHistoryEntry>>(),
    }),
  timeInfo: (query?: { from: string; to: string }) =>
    queryOptions({
      queryKey: ["milltime", "time-info", query?.from, query?.to],
      queryFn: async () => {
        return api
          .get("milltime/time-info", {
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
        "milltime",
        "time-entries",
        query?.from,
        query?.to,
        query?.unique,
      ],
      queryFn: async () => {
        return api
          .get("milltime/time-entries", {
            searchParams: query ?? {
              from: dayjs().startOf("month").format("YYYY-MM-DD"),
              to: dayjs().endOf("month").format("YYYY-MM-DD"),
            },
          })
          .json<Array<TimeEntry>>();
      },
    }),
};

export type TimerType = "Milltime" | "Standalone";

export type GetTimerResponse = {
  timer: TimerResponse | null;
};

/** Active timer response - unified for both Milltime and Standalone timers. */
export type TimerResponse = {
  timerType: TimerType;
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
  timerType: TimerType;
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
