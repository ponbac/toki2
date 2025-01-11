import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import dayjs from "dayjs";

export const milltimeQueries = {
  listProjects: (query?: { showAll: boolean }) =>
    queryOptions({
      queryKey: ["milltime", "projects", query?.showAll],
      queryFn: async () =>
        api
          .get("milltime/projects", {
            searchParams: query,
          })
          .json<Array<ProjectSearchItem>>(),
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
        api.get("milltime/timer").json<MilltimeTimer | DatabaseTimer>(),
    }),
  timerHistory: () =>
    queryOptions({
      queryKey: [...milltimeQueries.timerBaseKey, "history"],
      queryFn: async () =>
        api.get("milltime/timer-history").json<Array<DatabaseTimer>>(),
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

export type DatabaseTimer = {
  id: number;
  userId: number;
  startTime: string;
  endTime: string | null;
  projectId: string | null;
  projectName: string | null;
  activityId: string | null;
  activityName: string | null;
  note?: string | null;
  createdAt: string;
  timerType: "Standalone";
};

export type TimeInfo = {
  overtimes: Array<{
    key: string;
    value: number;
    label: string;
  }>;
  flexTimePreviousPeriod: number | null;
  flexTimePeriod: number | null;
  flexTimeCurrent: number;
  flexWithdrawal: number;
  scheduledPeriodTime: number;
  workedPeriodTime: number;
  absencePeriodTime: number;
  workedPeriodWithAbsenceTime: number;
  periodTimeLeft: number;
  mtinfoDetailRow: unknown[];
};

export type MilltimeTimer = {
  timerRegistrationId: string;
  projectRegistrationId: string;
  userId: string;
  projectId: string;
  activity: string;
  phaseId: string;
  planningTaskId: unknown;
  startTime: string;
  note: string;
  ticketData: unknown;
  internalNote: unknown;
  typeOf: unknown;
  attendanceLogId: string;
  variationId: unknown;
  projTimeHh: unknown;
  projTimeMm: unknown;
  difference: string;
  projectName: string;
  activityName: string;
  attributeValue: unknown;
  requireNote: unknown;
  favoriteType: number;
  projectNr: unknown;
  hours: number;
  seconds: number;
  minutes: number;
  projectRegistration: unknown;
  timerType: "Milltime";
};

export type ProjectSearchItem = {
  id: number;
  userId: string;
  projectId: string;
  projectName: string;
  projectNr: unknown;
  leaderName: string;
  planningType: number;
  isFavorite: boolean;
  customerNames: string;
  isMember: boolean;
  isLeader: boolean;
};

export type Activity = {
  userId: string;
  projectId: string;
  activity: string;
  activityName: string;
  variationId: unknown;
  absenceType: unknown;
  phaseId: string;
  phaseName: string;
  requireNote: boolean | null;
  phaseOrder: number;
  isFavorite: boolean;
  projPlanDescription: unknown;
  planningTaskId: unknown;
  planningTaskName: unknown;
  name: string;
  timeDistributionType: unknown;
  planningType: number;
};

export type TimeEntry = {
  registrationId: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
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
