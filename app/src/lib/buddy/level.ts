import type { Buddy } from "@/lib/api/contracts/buddy";

// The server's XP rules, which the app only explains and animates.

/** XP for each logged hour of a day. */
export const HOURS_XP_PER_HOUR = 60;

/** Logged hours a day that give XP; the rest give none. */
export const MAX_XP_HOURS = 10;

/** XP the user who started a battle gets for it, whatever the result. */
export const BATTLE_XP = 20;

/**
 * Total XP a buddy needs to reach `level`: `ceil(0.7 × (level − 1)³)` in whole
 * numbers, the server's curve. Lv 1 needs none and Lv 2 needs 1.
 */
export function buddyXpForLevel(level: number): number {
  const steps = Math.max(0, Math.floor(level) - 1);
  return Math.ceil((7 * steps ** 3) / 10);
}

/** How far a buddy is through its current level. */
export type BuddyLevelProgress = {
  /** Share of the way to the next level, 0–1; 1 at the maximum level. */
  readonly fraction: number;
  /** XP still needed for the next level, `null` at the maximum level. */
  readonly xpToNext: number | null;
};

/** Progress toward the next level from the XP bounds the server reports. */
export function buddyLevelProgress(
  buddy: Pick<Buddy, "xp" | "levelXp" | "nextLevelXp">,
): BuddyLevelProgress {
  if (buddy.nextLevelXp === null) {
    return { fraction: 1, xpToNext: null };
  }

  const span = buddy.nextLevelXp - buddy.levelXp;
  const fraction = span > 0 ? (buddy.xp - buddy.levelXp) / span : 0;
  return {
    fraction: Math.min(1, Math.max(0, fraction)),
    xpToNext: Math.max(0, buddy.nextLevelXp - buddy.xp),
  };
}
