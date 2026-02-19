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

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type MutationFn<T> = T extends (...args: any[]) => { mutate: infer M }
  ? M
  : never;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type MutationFnAsync<T> = T extends (...args: any[]) => {
  mutateAsync: infer M;
}
  ? M
  : never;

export const mutations = {
  ...differsMutations,
  ...repositoriesMutations,
  ...timeTrackingMutations,
  ...userMutations,
  ...workItemsMutations,
};
