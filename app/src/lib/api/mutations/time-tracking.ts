import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";
import { timeTrackingQueries } from "../queries/time-tracking";
import { useTimeTrackingActions } from "@/hooks/useTimeTrackingStore";

export const timeTrackingMutations = {
  useAuthenticate,
  useStartTimer,
  useStopTimer,
  useSaveTimer,
  useEditTimer,
  useEditProjectRegistration,
  useDeleteProjectRegistration,
  useCreateProjectRegistration,
};

function useAuthenticate(options?: DefaultMutationOptions<AuthenticateBody>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "authenticate"],
    mutationFn: (body: AuthenticateBody) =>
      api.post("time-tracking/authenticate", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: ["time-tracking"],
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useStartTimer(options?: DefaultMutationOptions<StartTimerPayload>) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();

  return useMutation({
    mutationKey: ["time-tracking", "startTimer"],
    mutationFn: (body: StartTimerPayload) =>
      api.post("time-tracking/timer", {
        json: body,
      }),
    ...options,
    onMutate: (vars) => {
      queryClient.resetQueries({
        queryKey: timeTrackingQueries.getTimer().queryKey,
      });
      options?.onMutate?.(vars);
    },
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
      setTimer({
        visible: true,
        state: "running",
        timeSeconds: 0,
      });

      options?.onSuccess?.(data, v, c);
    },
  });
}

function useStopTimer(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();

  return useMutation({
    mutationKey: ["time-tracking", "stopTimer"],
    mutationFn: () => api.delete("time-tracking/timer"),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });

      options?.onSuccess?.(data, v, c);
    },
  });
}

function useSaveTimer(options?: DefaultMutationOptions<SaveTimerPayload>) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();

  return useMutation({
    mutationKey: ["time-tracking", "saveTimer"],
    mutationFn: (body: SaveTimerPayload) =>
      api.put("time-tracking/timer", {
        json: {
          userNote: body.userNote,
        },
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.resetQueries({
        queryKey: timeTrackingQueries.getTimer().queryKey,
      });
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeInfo().queryKey.slice(0, 2),
      });
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntries().queryKey.slice(0, 2),
      });
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });

      options?.onSuccess?.(data, v, c);
    },
  });
}

function useEditTimer(options?: DefaultMutationOptions<EditTimerPayload>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "editTimer"],
    mutationFn: (body: EditTimerPayload) =>
      api.put("time-tracking/update-timer", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.getTimer().queryKey,
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useEditProjectRegistration(
  options?: DefaultMutationOptions<EditProjectRegistrationPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "editProjectRegistration"],
    mutationFn: (body: EditProjectRegistrationPayload) =>
      api.put("time-tracking/time-entries", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntries().queryKey.slice(0, 2),
      });
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeInfo().queryKey.slice(0, 2),
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useDeleteProjectRegistration(
  options?: DefaultMutationOptions<DeleteProjectRegistrationPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "deleteProjectRegistration"],
    mutationFn: (body: DeleteProjectRegistrationPayload) =>
      api.delete("time-tracking/time-entries", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntries().queryKey.slice(0, 2),
      });
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeInfo().queryKey.slice(0, 2),
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useCreateProjectRegistration(
  options?: DefaultMutationOptions<CreateProjectRegistrationPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "createProjectRegistration"],
    mutationFn: (body: CreateProjectRegistrationPayload) =>
      api.post("time-tracking/time-entries", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntries().queryKey.slice(0, 2),
      });
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeInfo().queryKey.slice(0, 2),
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

export const authenticateSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(1, "Password is required"),
});

export type AuthenticateBody = z.infer<typeof authenticateSchema>;

export type StartTimerPayload = {
  userNote?: string;
  projectId?: string;
  projectName?: string;
  activityId?: string;
  activityName?: string;
};

export type SaveTimerPayload = {
  userNote?: string;
};

export type EditTimerPayload = {
  userNote?: string;
  projectId?: string;
  projectName?: string;
  activityId?: string;
  activityName?: string;
  startTime?: string;
};

export type EditProjectRegistrationPayload = {
  projectRegistrationId: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string;
  endTime: string;
  regDay: string;
  weekNumber: number;
  userNote: string;
  originalRegDay?: string;
  originalProjectId?: string;
  originalActivityId?: string;
};

export type DeleteProjectRegistrationPayload = {
  projectRegistrationId: string;
};

export type UpdateTimeEntryPayload = {
  id: string;
  note: string;
  hours: number;
};

export type CreateProjectRegistrationPayload = {
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string; // ISO
  endTime: string; // ISO
  regDay: string; // YYYY-MM-DD
  weekNumber: number;
  userNote: string;
};
