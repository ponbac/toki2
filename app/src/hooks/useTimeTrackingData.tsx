import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { useTimeTrackingActions } from "./useTimeTrackingStore";

export const useTimeTrackingData = (options?: {
  projectId?: string;
  enabled?: boolean;
}) => {
  const isEnabled = options?.enabled ?? true;
  const { reset } = useTimeTrackingActions();
  const { data: connectionStatus } = useQuery({
    ...timeTrackingQueries.connectionStatus(),
    enabled: isEnabled,
  });
  const isAuthenticated = connectionStatus?.connected ?? false;

  const { data: projects, isLoading: isProjectsLoading } = useQuery({
    ...timeTrackingQueries.listProjects(),
    enabled: isAuthenticated && isEnabled,
  });

  const { data: activities, isLoading: isActivitiesLoading } = useQuery({
    ...timeTrackingQueries.listActivities(options?.projectId ?? ""),
    enabled: isAuthenticated && !!options?.projectId && isEnabled,
  });

  useEffect(() => {
    if (!isAuthenticated) {
      reset();
    }
  }, [isAuthenticated, reset]);

  return {
    projects,
    activities,
    isProjectsLoading,
    isActivitiesLoading,
    isAuthenticated,
    connectionStatus,
  };
};
