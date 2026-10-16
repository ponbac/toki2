import { queryOptions } from "@tanstack/react-query";
import { HTTPError } from "ky";
import {
  type LegacyBuddyChoice,
  forgetLegacyBuddyChoice,
  readLegacyBuddyChoice,
} from "@/lib/buddy/legacy-choice";
import { defaultBuddySpeciesId } from "@/lib/buddy/species";
import { api } from "../api";
import {
  type BuddyProfile,
  parseBattleDetail,
  parseBattleList,
  parseBuddyLeaderboard,
  parseBuddyOpponents,
  parseBuddyProfileResponse,
  parseBuddyTrainer,
  parseUpdatedBuddyProfile,
} from "../contracts/buddy";
import { parseUser } from "../contracts/user";

export type {
  BattleDetail,
  BattleEvent,
  BattleKind,
  BattleMon,
  BattleMove,
  BattleParticipant,
  BattleRecord,
  BattleResult,
  BattleSide,
  BattleSummary,
  BattleType,
  Buddy,
  BuddyLeaderboard,
  BuddyLeaderboardRow,
  BuddyOpponent,
  BuddyOpponents,
  BuddyProfile,
  BuddyProfileUpdate,
  BuddySlot,
  BuddyTeamMember,
  BuddyToday,
  BuddyTrainer,
} from "../contracts/buddy";

/**
 * Root of every buddy API query. Not `["buddy"]`: that holds the decoded
 * sprite atlases, which must survive invalidating the API reads.
 */
export const BUDDY_QUERY_KEY = ["buddies"] as const;

/** How often the reads that change through other users' battles are refetched. */
const BUDDY_POLL_INTERVAL_MS = 60 * 1000;

/** Query keys of the buddy API, for invalidation and cache updates. */
export const buddyQueryKeys = {
  profile: [...BUDDY_QUERY_KEY, "profile"] as const,
  opponents: [...BUDDY_QUERY_KEY, "opponents"] as const,
  trainer: [...BUDDY_QUERY_KEY, "trainer"] as const,
  /** The battle list; details live under `battle` so they are not refetched with it. */
  battles: [...BUDDY_QUERY_KEY, "battles"] as const,
  battle: (battleId: number) =>
    [...BUDDY_QUERY_KEY, "battle", battleId] as const,
  leaderboards: [...BUDDY_QUERY_KEY, "leaderboard"] as const,
  leaderboard: (month: string | undefined) =>
    [...BUDDY_QUERY_KEY, "leaderboard", month ?? "current"] as const,
};

/**
 * Enrols a user who answered the old, device-local intro, so their answer and
 * Pokemon carry over. `legacy` is what the device held before the profile was
 * fetched. Returns `null` when there is nothing to carry over or the server
 * did not take it; a transient failure keeps the stored answer for the next
 * load, a rejected one forgets it so the intro asks again.
 */
async function adoptLegacyBuddyChoice(
  legacy: LegacyBuddyChoice,
): Promise<BuddyProfile | null> {
  if (legacy.enabled === null) {
    return null;
  }
  // Forgotten since: another tab moved it over while this one fetched, so ask
  // again for the profile that tab created.
  if (readLegacyBuddyChoice().enabled === null) {
    return parseBuddyProfileResponse(await api.get("buddy").json<unknown>());
  }

  try {
    const leadSpeciesId =
      legacy.speciesId ??
      defaultBuddySpeciesId(parseUser(await api.get("me").json<unknown>()).email);
    const profile = parseUpdatedBuddyProfile(
      await api
        .put("buddy", { json: { enabled: legacy.enabled, leadSpeciesId } })
        .json<unknown>(),
    );
    forgetLegacyBuddyChoice();
    return profile;
  } catch (error) {
    const status = error instanceof HTTPError ? error.response.status : null;
    // The server refused this choice for good, as opposed to being unreachable or failing.
    if (status === 400 || status === 409) {
      forgetLegacyBuddyChoice();
    }
    console.warn(
      `Could not move the timer buddy choice to the server (${status ?? "no response"})`,
    );
    return null;
  }
}

let adoptionInFlight: Promise<BuddyProfile | null> | null = null;

/** A refetch that overtakes a running adoption waits for it instead of enrolling twice. */
function adoptLegacyBuddyChoiceOnce(
  legacy: LegacyBuddyChoice,
): Promise<BuddyProfile | null> {
  adoptionInFlight ??= adoptLegacyBuddyChoice(legacy).finally(() => {
    adoptionInFlight = null;
  });
  return adoptionInFlight;
}

/** Query definitions for buddy profiles, battles and the leaderboard. */
export const buddyApiQueries = {
  /**
   * The current user's buddy profile, `null` until they enrol. Fetching it
   * also syncs today's logged hours on the server, at most once a minute, so
   * it is not refetched more often than that. A first fetch that finds no
   * profile moves a choice stored on this device by the old intro to the
   * server, once.
   */
  profile: () =>
    queryOptions({
      queryKey: buddyQueryKeys.profile,
      queryFn: async ({ signal }) => {
        // Read first: another tab can move the choice over during the fetch.
        const legacy = readLegacyBuddyChoice();
        const profile = parseBuddyProfileResponse(
          await api.get("buddy", { signal }).json<unknown>(),
        );
        return profile ?? (await adoptLegacyBuddyChoiceOnce(legacy));
      },
      staleTime: 60 * 1000,
    }),
  /**
   * Colleagues to battle and the hourly cooldown. Needs an enabled profile
   * (409 otherwise). Polled, as colleagues turn their buddies on and off and
   * battle each other.
   */
  opponents: () =>
    queryOptions({
      queryKey: buddyQueryKeys.opponents,
      queryFn: async ({ signal }) =>
        parseBuddyOpponents(
          await api.get("buddy/opponents", { signal }).json<unknown>(),
        ),
      refetchInterval: BUDDY_POLL_INTERVAL_MS,
    }),
  /** Today's daily trainer and the current user's result against it. */
  trainer: () =>
    queryOptions({
      queryKey: buddyQueryKeys.trainer,
      queryFn: async ({ signal }) =>
        parseBuddyTrainer(
          await api.get("buddy/trainer", { signal }).json<unknown>(),
        ),
    }),
  /**
   * The current user's battles of the last 30 days, newest first, at most 50.
   * Polled, so a battle a colleague starts against the user shows up without
   * a push notification.
   */
  battles: () =>
    queryOptions({
      queryKey: buddyQueryKeys.battles,
      queryFn: async ({ signal }) =>
        parseBattleList(
          await api.get("buddy/battles", { signal }).json<unknown>(),
        ),
      refetchInterval: BUDDY_POLL_INTERVAL_MS,
    }),
  /**
   * One battle with its teams and log, for the replay. A battle never changes
   * apart from being seen, so it is never refetched. 404 unless the current
   * user took part.
   */
  battle: (battleId: number) =>
    queryOptions({
      queryKey: buddyQueryKeys.battle(battleId),
      queryFn: async ({ signal }) =>
        parseBattleDetail(
          await api.get(`buddy/battles/${battleId}`, { signal }).json<unknown>(),
        ),
      staleTime: Infinity,
    }),
  /**
   * The leaderboard of a local calendar month, `YYYY-MM`, or of the server's
   * current month when `month` is omitted. 400 for a malformed month.
   */
  leaderboard: (month?: string) =>
    queryOptions({
      queryKey: buddyQueryKeys.leaderboard(month),
      queryFn: async ({ signal }) =>
        parseBuddyLeaderboard(
          await api
            .get("buddy/leaderboard", {
              signal,
              searchParams: month === undefined ? undefined : { month },
            })
            .json<unknown>(),
        ),
    }),
};
