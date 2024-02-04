import { useMutation, useQueryClient } from "@tanstack/react-query";
import { queries } from "../queries/queries";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { z } from "zod";

export const repositoriesMutations = { useAddRepository };

function useAddRepository(options?: DefaultMutationOptions<AddRepositoryBody>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["addRepository"],
    mutationFn: (body: AddRepositoryBody) =>
      api.post("repositories", {
        json: body,
      }),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries(queries.differs());
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

export const addRepositorySchema = z.object({
  organization: z.string().min(1, "Organization is required"),
  project: z.string().min(1, "Project is required"),
  repoName: z.string().min(1, "Repository name is required"),
  token: z.string().min(1, "Token is required"),
});

export type AddRepositoryBody = z.infer<typeof addRepositorySchema>;
