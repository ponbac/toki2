import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";
import { useMilltimeActions } from "@/hooks/useMilltimeContext";
import { milltimeQueries, TimerType } from "../queries/milltime";

export const milltimeMutations = {
  useAuthenticate,
  useStartTimer,
  useStartStandaloneTimer,
  useStopTimer,
  useSaveTimer,
  useEditTimer,
  useEditStandaloneTimer,
};

function useAuthenticate(options?: DefaultMutationOptions<AuthenticateBody>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["milltime", "authenticate"],
    mutationFn: (body: AuthenticateBody) =>
      api.post("milltime/authenticate", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: ["milltime"],
      });
      options?.onSuccess?.(data, v, c);
    },
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
    onMutate: (vars) => {
      queryClient.resetQueries({
        queryKey: milltimeQueries.getTimer().queryKey,
      });
      options?.onMutate?.(vars);
    },
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timerBaseKey,
      });
      setTimer({
        visible: true,
        state: "running",
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

function useStartStandaloneTimer(
  options?: DefaultMutationOptions<StartStandaloneTimerPayload>,
) {
  const queryClient = useQueryClient();
  const { setTimer } = useMilltimeActions();

  return useMutation({
    mutationKey: ["milltime", "startStandaloneTimer"],
    mutationFn: (body: StartStandaloneTimerPayload) =>
      api.post("milltime/timer/standalone", {
        json: body,
      }),
    ...options,
    onMutate: (vars) => {
      queryClient.resetQueries({
        queryKey: milltimeQueries.getTimer().queryKey,
      });
      options?.onMutate?.(vars);
    },
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.timerBaseKey,
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

function useStopTimer(
  options?: DefaultMutationOptions<{ timerType: TimerType }>,
) {
  const queryClient = useQueryClient();
  const { reset, setTimer } = useMilltimeActions();

  return useMutation({
    mutationKey: ["milltime", "stopTimer"],
    mutationFn: (body: { timerType: TimerType }) =>
      body.timerType === "Milltime"
        ? api.delete("milltime/timer")
        : api.delete("milltime/timer/standalone"),
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
      body.timerType === "Milltime"
        ? api.put("milltime/timer", {
            json: {
              user_note: body.userNote,
            },
          })
        : api.put("milltime/timer/standalone", {
            json: {
              user_note: body.userNote,
            },
          }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.resetQueries({
        queryKey: milltimeQueries.getTimer().queryKey,
      });
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
        queryKey: milltimeQueries.getTimer().queryKey,
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useEditStandaloneTimer(
  options?: DefaultMutationOptions<EditStandaloneTimerPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["milltime", "editStandaloneTimer"],
    mutationFn: (body: EditStandaloneTimerPayload) =>
      api.put("milltime/update-timer/standalone", {
        json: body,
      }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: milltimeQueries.getTimer().queryKey,
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
  inputTime?: string;
  projTime?: string;
};

export type StartStandaloneTimerPayload = {
  userNote?: string;
};
export type SaveTimerPayload = {
  timerType: TimerType;
  userNote?: string;
};

export type EditTimerPayload = {
  userNote: string;
};

export type EditStandaloneTimerPayload = {
  userNote: string;
  projectId?: string;
  projectName?: string;
  activityId?: string;
  activityName?: string;
};
