import type { BuddyLeaderboardRow } from "@/lib/api/queries/buddy";
import { describeRecord, formatRecord, plural } from "./format";

/** The leaderboard's tabs: most wins, best win rate and most daily trainer wins. */
export const LEADERBOARD_TABS = ["wins", "rate", "trainer"] as const;
export type LeaderboardTab = (typeof LEADERBOARD_TABS)[number];

export const LEADERBOARD_TAB_LABELS: Readonly<Record<LeaderboardTab, string>> =
  {
    wins: "Wins",
    rate: "Win rate",
    trainer: "Daily trainer",
  };

/** Colleague battles a month needs before its win rate is ranked. */
export const MIN_RATED_BATTLES = 5;

/** A row on one tab, with its place and the figure the tab ranks by. */
export type LeaderboardEntry = {
  readonly row: BuddyLeaderboardRow;
  /** 1-based; rows that tie on everything the tab ranks by share a place. */
  readonly rank: number;
  readonly score: string;
  /** The score in words, for a tooltip and screen readers. */
  readonly description: string;
};

/** Colleague battles the user started in the month, whatever the result. */
export function battleCount(row: BuddyLeaderboardRow): number {
  return row.wins + row.losses + row.draws;
}

type TabRules = {
  /** Whether a row is on the tab at all. */
  readonly includes: (row: BuddyLeaderboardRow) => boolean;
  /** Negative when `a` ranks above `b`; 0 when they share a place. */
  readonly compare: (a: BuddyLeaderboardRow, b: BuddyLeaderboardRow) => number;
  readonly score: (row: BuddyLeaderboardRow) => string;
  readonly description: (row: BuddyLeaderboardRow) => string;
};

/** Compares win rates exactly, without dividing. */
function compareRates(a: BuddyLeaderboardRow, b: BuddyLeaderboardRow): number {
  return b.wins * battleCount(a) - a.wins * battleCount(b);
}

function winRate(row: BuddyLeaderboardRow): number {
  const battles = battleCount(row);
  return battles > 0 ? row.wins / battles : 0;
}

const TAB_RULES: Readonly<Record<LeaderboardTab, TabRules>> = {
  wins: {
    includes: (row) => battleCount(row) > 0,
    // Most wins, then the fewest losses.
    compare: (a, b) => b.wins - a.wins || a.losses - b.losses,
    score: formatRecord,
    description: describeRecord,
  },
  rate: {
    includes: (row) => battleCount(row) >= MIN_RATED_BATTLES,
    // The best rate, then the one that held up over more battles.
    compare: (a, b) => compareRates(a, b) || battleCount(b) - battleCount(a),
    score: (row) => `${Math.round(winRate(row) * 100)}%`,
    description: (row) =>
      `${plural(row.wins, "win")} in ${plural(battleCount(row), "battle")}`,
  },
  trainer: {
    includes: (row) => row.trainerWins > 0,
    compare: (a, b) => b.trainerWins - a.trainerWins,
    score: (row) => plural(row.trainerWins, "day"),
    description: (row) =>
      `Beat the daily trainer on ${plural(row.trainerWins, "day")}`,
  },
};

/**
 * The rows of one tab, best first. Ties keep the same place and are listed by
 * name, and the place after a tie is skipped: 1, 2, 2, 4.
 */
export function rankLeaderboard(
  rows: readonly BuddyLeaderboardRow[],
  tab: LeaderboardTab,
): readonly LeaderboardEntry[] {
  const rules = TAB_RULES[tab];
  const sorted = rows
    .filter(rules.includes)
    .sort(
      (a, b) =>
        rules.compare(a, b) || a.fullName.localeCompare(b.fullName, "en"),
    );

  return sorted.map((row) => {
    const firstTied = sorted.findIndex(
      (other) => rules.compare(other, row) === 0,
    );
    return {
      row,
      rank: firstTied + 1,
      score: rules.score(row),
      description: rules.description(row),
    };
  });
}

/** Users who battled in the month, but too few times for the win rate tab. */
export function unratedRows(
  rows: readonly BuddyLeaderboardRow[],
): readonly BuddyLeaderboardRow[] {
  return rows
    .filter((row) => {
      const battles = battleCount(row);
      return battles > 0 && battles < MIN_RATED_BATTLES;
    })
    .sort(
      (a, b) =>
        battleCount(b) - battleCount(a) ||
        a.fullName.localeCompare(b.fullName, "en"),
    );
}
