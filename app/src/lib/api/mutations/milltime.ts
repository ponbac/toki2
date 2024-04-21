import { useMutation } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";

export const milltimeMutations = { useAuthenticate };

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

export const authenticateSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(1, "Password is required"),
});

export type AuthenticateBody = z.infer<typeof authenticateSchema>;
