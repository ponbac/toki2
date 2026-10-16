import {
  type QueryClient,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "../api";
import {
  parseBattleDetail,
  parseUpdatedBuddyProfile,
} from "../contracts/buddy";
import {
  type BattleDetail,
  type BuddyProfile,
  type BuddyProfileUpdate,
  buddyApiQueries,
  buddyQueryKeys,
} from "../queries/buddy";
import type { DefaultMutationOptions } from "./mutations";

/** Input for battling a colleague's buddies. */
export type StartBattleVars = { opponentId: number };
/** Input for marking a battle the current user defended as seen. */
export type MarkBattleSeenVars = { battleId: number };

/** Mutation hooks for buddy profiles and battles. */
export const buddyMutations = {
  useUpdateBuddyProfile,
  useStartBattle,
  useStartTrainerBattle,
  useMarkBattleSeen,
};

/**
 * Changes the current user's buddy profile, enrolling them on the first call.
 * The returned profile replaces the cached one; reads that show the user's
 * team or depend on it being enabled are refreshed.
 */
export function useUpdateBuddyProfile(
  options?: DefaultMutationOptions<BuddyProfileUpdate, BuddyProfile>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: [...buddyQueryKeys.profile, "update"],
    mutationFn: async (update: BuddyProfileUpdate) =>
      parseUpdatedBuddyProfile(
        await api.put("buddy", { json: update }).json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      // A fetch that started before the update would put the old profile back.
      await queryClient.cancelQueries({ queryKey: buddyQueryKeys.profile });
      queryClient.setQueryData(buddyApiQueries.profile().queryKey, data);
      // Not awaited, so the controls unlock as soon as the change is saved.
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.opponents });
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.trainer });
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.leaderboards });
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/**
 * Battles a colleague's buddies and returns the battle to replay. A refusal
 * (409 not available or battled today, 429 cooldown) also refreshes the
 * opponents, whose cached state was evidently out of date.
 */
export function useStartBattle(
  options?: DefaultMutationOptions<StartBattleVars, BattleDetail>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: [...buddyQueryKeys.battles, "start"],
    mutationFn: async ({ opponentId }: StartBattleVars) =>
      parseBattleDetail(
        await api.post("buddy/battles", { json: { opponentId } }).json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      queryClient.setQueryData(buddyApiQueries.battle(data.id).queryKey, data);
      invalidateAfterBattle(queryClient);
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.opponents });
      await options?.onSuccess?.(data, vars, ctx);
    },
    onError: async (error, vars, ctx) => {
      // Not awaited, so the refusal is shown at once.
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.opponents });
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.profile });
      await options?.onError?.(error, vars, ctx);
    },
  });
}

/**
 * Fights today's trainer and returns the battle to replay. A refusal (409
 * already fought today) also refreshes the trainer.
 */
export function useStartTrainerBattle(
  options?: DefaultMutationOptions<void, BattleDetail>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: [...buddyQueryKeys.trainer, "battle"],
    mutationFn: async () =>
      parseBattleDetail(
        await api.post("buddy/trainer/battles", { json: {} }).json<unknown>(),
      ),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      queryClient.setQueryData(buddyApiQueries.battle(data.id).queryKey, data);
      invalidateAfterBattle(queryClient);
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.trainer });
      await options?.onSuccess?.(data, vars, ctx);
    },
    onError: async (error, vars, ctx) => {
      // Not awaited, so the refusal is shown at once.
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.trainer });
      queryClient.invalidateQueries({ queryKey: buddyQueryKeys.profile });
      await options?.onError?.(error, vars, ctx);
    },
  });
}

/**
 * Marks a battle the current user defended as seen. The cached battle is
 * updated in place, so a replay showing it keeps its log; the list refetches.
 */
export function useMarkBattleSeen(
  options?: DefaultMutationOptions<MarkBattleSeenVars>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: [...buddyQueryKeys.battles, "seen"],
    mutationFn: async ({ battleId }: MarkBattleSeenVars) =>
      api.post(`buddy/battles/${battleId}/seen`),
    ...options,
    onSuccess: async (data, vars, ctx) => {
      queryClient.setQueryData(
        buddyApiQueries.battle(vars.battleId).queryKey,
        (battle) => (battle ? { ...battle, seen: true } : battle),
      );
      await queryClient.invalidateQueries({
        queryKey: buddyQueryKeys.battles,
      });
      await options?.onSuccess?.(data, vars, ctx);
    },
  });
}

/**
 * A started battle gives XP and counts on the leaderboard. Not awaited, so the
 * caller opens the replay as soon as the battle is stored: refetching the
 * profile can wait on a sync of logged hours.
 */
function invalidateAfterBattle(queryClient: QueryClient): void {
  queryClient.invalidateQueries({ queryKey: buddyQueryKeys.profile });
  queryClient.invalidateQueries({ queryKey: buddyQueryKeys.battles });
  queryClient.invalidateQueries({ queryKey: buddyQueryKeys.leaderboards });
}
