import { useQuery } from "@tanstack/react-query";
import * as React from "react";
import {
  type Buddy,
  type BuddyProfile,
  type BuddyTeamMember,
  buddyApiQueries,
} from "@/lib/api/queries/buddy";
import { type User, userQueries } from "@/lib/api/queries/user";
import { readLegacyBuddyChoice } from "./legacy-choice";
import {
  type BuddySpeciesId,
  buddySpeciesName,
  defaultBuddySpeciesId,
} from "./species";

const selectEnabled = (profile: BuddyProfile | null) =>
  profile?.enabled ?? false;

const selectEmail = (me: User) => me.email;

/**
 * Whether the current user's buddies are switched on. False while the
 * profile loads, when it fails to load, and before the user enrols.
 */
export function useBuddiesEnabled(): boolean {
  const { data: enabled } = useQuery({
    ...buddyApiQueries.profile(),
    select: selectEnabled,
  });
  return enabled ?? false;
}

/** The Pokemon a user starts with when they enrol. */
export type StartingBuddy = {
  /** `null` while the avatar seed loads. */
  readonly speciesId: BuddySpeciesId | null;
  /** Picked on this device before buddies were stored on the server; otherwise the avatar's. */
  readonly picked: boolean;
};

/**
 * The Pokemon a user who has not enrolled yet starts with: the one they picked
 * on this device before buddies were stored on the server, when moving it
 * over has not worked yet, or else their avatar's.
 */
export function useStartingBuddy(): StartingBuddy {
  // Read once: the pick is only forgotten when the user enrols.
  const [pickedSpeciesId] = React.useState(
    () => readLegacyBuddyChoice().speciesId,
  );
  const { data: email } = useQuery({
    ...userQueries.me(),
    select: selectEmail,
    staleTime: Infinity,
  });

  if (pickedSpeciesId !== null) {
    return { speciesId: pickedSpeciesId, picked: true };
  }
  return {
    speciesId: email === undefined ? null : defaultBuddySpeciesId(email),
    picked: false,
  };
}

/** The profile's buddies in slot order: the lead, then the partner when there is one. */
export function buddyTeam(profile: BuddyProfile): readonly Buddy[] {
  return profile.partner ? [profile.lead, profile.partner] : [profile.lead];
}

/** A team on one line, such as "Pikachu Lv 31 · Lucario Lv 12". */
export function formatBuddyTeam(
  team: ReadonlyArray<Pick<BuddyTeamMember, "speciesId" | "level">>,
): string {
  return team
    .map((member) => `${buddySpeciesName(member.speciesId)} Lv ${member.level}`)
    .join(" · ");
}
