import { MutationOptions } from "@tanstack/react-query";
import { differsMutations } from "./differs";
import { repositoriesMutations } from "./repositories";
import { timeTrackingMutations } from "./time-tracking";
import { userMutations } from "./user";
import { workItemsMutations } from "./workItems";

export type DefaultMutationOptions<
  TVars = void,
  TResponse = Response,
  TErr = unknown,
  TContext = unknown,
> = Omit<
  MutationOptions<TResponse, TErr, TVars, TContext>,
  "mutationKey" | "mutationFn"
>;

export const mutations = {
  ...differsMutations,
  ...repositoriesMutations,
  ...timeTrackingMutations,
  ...userMutations,
  ...workItemsMutations,
};
