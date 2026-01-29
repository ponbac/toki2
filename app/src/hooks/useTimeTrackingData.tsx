/* eslint-disable react-compiler/react-compiler */
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import { useTimeTrackingActions } from "./useTimeTrackingStore";
import { useTimeTrackingIsAuthenticated } from "./useTimeTrackingStore";

export const useTimeTrackingData = (options?: {
  projectId?: string;
  enabled?: boolean;
}) => {
  const isAuthenticated = useTimeTrackingIsAuthenticated();
  const { reset } = useTimeTrackingActions();

  const { data: projects } = useQuery({
    ...timeTrackingQueries.listProjects(),
    enabled: isAuthenticated && options?.enabled,
  });

  const { data: activities } = useQuery({
    ...timeTrackingQueries.listActivities(options?.projectId ?? ""),
    enabled: isAuthenticated && !!options?.projectId && options?.enabled,
  });

  useEffect(() => {
    if (!isAuthenticated) {
      reset();
    }
  }, [isAuthenticated, reset]);

  const result = useMemo(() => {
    return {
      projects,
      activities,
      isAuthenticated,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options?.projectId, projects, activities, isAuthenticated]);

  return result;
};
