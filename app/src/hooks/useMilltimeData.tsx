import { milltimeQueries } from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";
import {
  useMilltimeActions,
  useMilltimeIsAuthenticated,
} from "./useMilltimeContext";

export const useMilltimeData = (options?: { projectId?: string }) => {
  const isAuthenticated = useMilltimeIsAuthenticated();
  const { reset } = useMilltimeActions();

  const { data: projects } = useQuery({
    ...milltimeQueries.listProjects(),
    enabled: isAuthenticated,
  });

  const { data: activities } = useQuery({
    ...milltimeQueries.listActivities(options?.projectId ?? ""),
    enabled: isAuthenticated && !!options?.projectId,
  });

  if (!isAuthenticated) {
    reset();
  }

  return {
    projects,
    activities,
    isAuthenticated,
  };
};
