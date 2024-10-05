import { milltimeQueries } from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";
import {
  useMilltimeActions,
  useMilltimeIsAuthenticated,
} from "./useMilltimeContext";
import { useMemo } from "react";

export const useMilltimeData = (options?: {
  projectId?: string;
  enabled?: boolean;
}) => {
  const isAuthenticated = useMilltimeIsAuthenticated();
  const { reset } = useMilltimeActions();

  const { data: projects } = useQuery({
    ...milltimeQueries.listProjects(),
    enabled: isAuthenticated && options?.enabled,
  });

  const { data: activities } = useQuery({
    ...milltimeQueries.listActivities(options?.projectId ?? ""),
    enabled: isAuthenticated && !!options?.projectId && options?.enabled,
  });

  if (!isAuthenticated) {
    reset();
  }

  const result = useMemo(() => {
    console.log("projects", projects);
    console.log("activities", activities);
    return {
      projects,
      activities,
      isAuthenticated,
    };
  }, [options?.projectId, projects, activities, isAuthenticated]);

  return result;
};
