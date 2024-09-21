import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";
import { useMilltimeActions } from "@/hooks/useMilltimeContext";
import { milltimeQueries } from "../queries/milltime";

export const milltimeMutations = {
  useAuthenticate,
  useStartTimer,
  useStopTimer,
  useSaveTimer,
  useEditTimer,
};

function useAuthenticate(options?: DefaultMutationOptions<AuthenticateBody>) {
  return useMutation({
    mutationKey: ["milltime", "authenticate"],
    mutationFn: (body: AuthenticateBody) =>
      api.post("milltime/authenticate", {
        json: body,
      }),
    ...options,
  });
}

function useStartTimer(options?: DefaultMutationOptions<StartTimerPayload>) {
  const queryClient = useQueryClient();
  const { reset, setTimer } = useMilltimeActions();

  return useMutation({
    mutationKey: ["milltime", "startTimer"],
    mutationFn: (body: StartTimerPayload) =>
      api.post("milltime/timer", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timerBaseKey,
      });
      setTimer({
        state: "running",
        visible: true,
        timeSeconds: 0,
      });

      options?.onSuccess?.(data, v, c);
    },
    onError: (e, v, c) => {
      reset();
      options?.onError?.(e, v, c);
    },
  });
}

function useStopTimer(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();
  const { reset, setTimer } = useMilltimeActions();

  return useMutation({
    mutationKey: ["milltime", "stopTimer"],
    mutationFn: () => api.delete("milltime/timer"),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timerBaseKey,
      });
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });

      options?.onSuccess?.(data, v, c);
    },
    onError: (e, v, c) => {
      reset();
      options?.onError?.(e, v, c);
    },
  });
}

function useSaveTimer(options?: DefaultMutationOptions<SaveTimerPayload>) {
  const queryClient = useQueryClient();
  const { reset, setTimer } = useMilltimeActions();

  return useMutation({
    mutationKey: ["milltime", "saveTimer"],
    mutationFn: (body: SaveTimerPayload) =>
      api.put("milltime/timer", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timerBaseKey,
      });
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timeInfo().queryKey.slice(0, 2),
      });
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timeEntries().queryKey.slice(0, 2),
      });
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });

      options?.onSuccess?.(data, v, c);
    },
    onError: (e, v, c) => {
      reset();
      options?.onError?.(e, v, c);
    },
  });
}

function useEditTimer(options?: DefaultMutationOptions<EditTimerPayload>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["milltime", "editTimer"],
    mutationFn: (body: EditTimerPayload) =>
      api.put("milltime/update-timer", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timerBaseKey,
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
  activity: string;
  activityName: string;
  projectId: string;
  projectName: string;
  userNote?: string;
  regDay: string;
  weekNumber: number;
};

export type SaveTimerPayload = {
  userNote?: string;
};

export type EditTimerPayload = {
  userNote: string;
};
