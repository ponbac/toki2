import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";

export const milltimeQueries = {
  listProjects: () =>
    queryOptions({
      queryKey: ["milltime", "projects"],
      queryFn: async () =>
        api.get("milltime/projects").json<Array<ProjectSearchItem>>(),
    }),
};

export type ProjectSearchItem = {
  id: number;
  user_id: string;
  project_id: string;
  project_name: string;
  project_nr: unknown;
  leader_name: string;
  planning_type: number;
  is_favorite: boolean;
  customer_names: string;
  is_member: boolean;
  is_leader: boolean;
};
