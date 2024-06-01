import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

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
  getTimer: () =>
    queryOptions({
      queryKey: ["milltime", "timer"],
      queryFn: async () => api.get("milltime/timer").json<Timer>(),
    }),
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
