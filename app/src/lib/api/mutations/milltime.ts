import { useMutation } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";

export const milltimeMutations = {
  useAuthenticate,
  useStartTimer,
  useStopTimer,
  useSaveTimer,
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
  return useMutation({
    mutationKey: ["milltime", "startTimer"],
    mutationFn: (body: StartTimerPayload) =>
      api.post("milltime/timer", {
        json: body,
      }),
    ...options,
  });
}

function useStopTimer(options?: DefaultMutationOptions<void>) {
  return useMutation({
    mutationKey: ["milltime", "stopTimer"],
    mutationFn: () => api.delete("milltime/timer"),
    ...options,
  });
}

function useSaveTimer(options?: DefaultMutationOptions<void>) {
  return useMutation({
    mutationKey: ["milltime", "saveTimer"],
    mutationFn: () => api.put("milltime/timer"),
    ...options,
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
  userId: string;
  userNote?: string;
  regDay: string;
  weekNumber: number;
};
