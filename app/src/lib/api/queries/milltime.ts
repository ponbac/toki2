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
