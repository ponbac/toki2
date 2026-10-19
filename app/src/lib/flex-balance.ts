import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import { timeTrackingQueries } from "./api/queries/time-tracking";
import { formatHoursMinutes } from "./utils";

/** The flex balance through today; `null` when the provider keeps none. */
export function useFlexBalance() {
  return useQuery({
    ...timeTrackingQueries.flexBalance(dayjs().format("YYYY-MM-DD")),
    staleTime: 5 * 60 * 1000,
  });
}

export function formatSignedHours(hours: number) {
  return `${hours > 0 ? "+" : ""}${formatHoursMinutes(hours)}`;
}

export function signedHoursColor(hours: number) {
  return hours > 0 ? "text-emerald-500" : hours < 0 ? "text-amber-500" : "";
}
