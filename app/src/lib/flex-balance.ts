import { useQuery } from "@tanstack/react-query";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { timeTrackingQueries } from "./api/queries/time-tracking";
import { flexBreakdown } from "./flex-breakdown";
import { formatHoursMinutes } from "./utils";

/**
 * The flex balance split around `today`, with the running timer counted as
 * today's, so every view shows the same numbers. `breakdown` is `null` when
 * the provider keeps no balance and `undefined` while it loads.
 */
export function useFlexBreakdown(today: string) {
  const { data: balance } = useQuery({
    ...timeTrackingQueries.flexBalanceBeforeToday(today),
    staleTime: 5 * 60 * 1000,
  });
  const { data: todayInfo } = useQuery({
    ...timeTrackingQueries.timeInfo({ from: today, to: today }),
    staleTime: 60 * 1000,
  });
  const timerSeconds = useTimeTrackingTimer().timeSeconds ?? 0;

  return {
    breakdown:
      balance && flexBreakdown(balance, todayInfo, timerSeconds / 3600),
    todayInfo,
    timerSeconds,
  };
}

export function formatSignedHours(hours: number) {
  return `${hours > 0 ? "+" : ""}${formatHoursMinutes(hours)}`;
}

export function signedHoursColor(hours: number) {
  return hours > 0 ? "text-emerald-500" : hours < 0 ? "text-amber-500" : "";
}
