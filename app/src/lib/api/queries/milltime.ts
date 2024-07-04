import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import dayjs from "dayjs";

export const milltimeQueries = {
  listProjects: () =>
    queryOptions({
      queryKey: ["milltime", "projects"],
      queryFn: async () =>
        api.get("milltime/projects").json<Array<ProjectSearchItem>>(),
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
      queryKey: [...milltimeQueries.timerBaseKey],
      queryFn: async () => api.get("milltime/timer").json<Timer>(),
    }),
  timerHistory: () =>
    queryOptions({
      queryKey: [...milltimeQueries.timerBaseKey, "history"],
      queryFn: async () =>
        api.get("milltime/timer-history").json<Array<TimerHistory>>(),
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
};

export type TimerHistory = {
  id: number;
  userId: number;
  startTime: string;
  endTime: string | null;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  note: string;
  createdAt: string;
};

export type TimeInfo = {
  overtimes: Array<{
    key: string;
    value: number;
    label: string;
  }>;
  flexTimePreviousPeriod: number | null;
  flexTimePeriod: number;
  flexTimeCurrent: number;
  flexWithdrawal: number;
  scheduledPeriodTime: number;
  workedPeriodTime: number;
  absencePeriodTime: number;
  workedPeriodWithAbsenceTime: number;
  periodTimeLeft: number;
  mtinfoDetailRow: unknown[];
};

export type Timer = {
  timerRegistrationId: string;
  projectRegistrationId: string;
  userId: string;
  projectId: string;
  activity: string;
  phaseId: string;
  planningTaskId: unknown;
  startTime: string;
  userNote: string;
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
