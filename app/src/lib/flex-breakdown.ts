import dayjs from "dayjs";
import type { FlexBalance, WeeklyStats } from "./api/queries/time-tracking";

/*
 * The flex balance split into what was booked before today and today's
 * hours so far. Kept apart from the queries so it can be tested without the
 * HTTP client.
 */

/** The balance before today, or through today when the provider has
 * already settled today. */
export type FlexBalanceBeforeToday = {
  balance: FlexBalance;
  includesToday: boolean;
};

/** A flex balance request's outcome: the balance, no balance kept for the
 * user, or a day before the one the provider has settled through. */
export type FlexBalanceReply = FlexBalance | "none" | "before-settlement";

/**
 * Loads the balance through yesterday. A provider that has settled through
 * today or later rejects yesterday, so the balance through today stands in
 * for it. `null` when no balance can be shown.
 */
export async function loadFlexBalanceBeforeToday(
  today: string,
  balanceThrough: (through: string) => Promise<FlexBalanceReply>,
): Promise<FlexBalanceBeforeToday | null> {
  const yesterday = dayjs(today).subtract(1, "day").format("YYYY-MM-DD");
  const beforeToday = await balanceThrough(yesterday);
  if (beforeToday === "none") return null;
  if (beforeToday !== "before-settlement") {
    return { balance: beforeToday, includesToday: false };
  }

  const throughToday = await balanceThrough(today);
  return typeof throughToday === "string"
    ? null
    : { balance: throughToday, includesToday: true };
}

export type FlexBreakdown = {
  settled: { hours: number; through: string };
  /** The estimate after the settled balance; `null` when no day falls between. */
  earlier: { from: string; through: string; hours: number } | null;
  /** `null` when the provider has already settled today. */
  beforeToday: number | null;
  /** Today's flex so far with the running timer; `null` while today's hours
   * load, when today is settled, or when nothing is scheduled or booked. */
  today: { hours: number; coveredHours: number; scheduledHours: number } | null;
  /** The running timer on its own row, only when today is settled; otherwise
   * today's row holds it. */
  timerHours: number | null;
  /** The balance if the running timer were saved now; `null` while today's
   * hours load. */
  nowHours: number | null;
};

export function flexBreakdown(
  { balance, includesToday }: FlexBalanceBeforeToday,
  todayInfo: WeeklyStats | undefined,
  unbookedHours: number,
): FlexBreakdown {
  const settled = {
    hours: balance.settledHours,
    through: balance.settledThrough,
  };
  const earlier =
    balance.settledThrough < balance.through
      ? {
          from: dayjs(balance.settledThrough)
            .add(1, "day")
            .format("YYYY-MM-DD"),
          through: balance.through,
          hours: balance.estimatedHours,
        }
      : null;

  if (includesToday) {
    return {
      settled,
      earlier,
      beforeToday: null,
      today: null,
      timerHours: unbookedHours > 0 ? unbookedHours : null,
      nowHours: balance.balanceHours + unbookedHours,
    };
  }

  if (!todayInfo) {
    return {
      settled,
      earlier,
      beforeToday: balance.balanceHours,
      today: null,
      timerHours: null,
      nowHours: null,
    };
  }

  const todayHours = todayInfo.periodFlexHours + unbookedHours;
  const coveredHours = todayInfo.coveredHours + unbookedHours;
  return {
    settled,
    earlier,
    beforeToday: balance.balanceHours,
    today:
      todayInfo.scheduledHours > 0 || coveredHours > 0
        ? {
            hours: todayHours,
            coveredHours,
            scheduledHours: todayInfo.scheduledHours,
          }
        : null,
    timerHours: null,
    nowHours: balance.balanceHours + todayHours,
  };
}
