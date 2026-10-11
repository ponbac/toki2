import type {
  AiCost,
  AiProjectAttribution,
  AiTokenCounts,
} from "@/lib/api/queries/ai-usage-report";

const usd = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
const compact = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
});
const whole = new Intl.NumberFormat("en-US");
const percent = new Intl.NumberFormat("en-US", {
  style: "percent",
  maximumFractionDigits: 1,
});

/** A USD amount with cents; a positive amount below one cent reads `<$0.01`. */
export function formatUsd(amount: number): string {
  return amount > 0 && amount < 0.005 ? "<$0.01" : usd.format(amount);
}

/**
 * An estimated cost. A partial cost reads as its known subtotal plus
 * "unknown", or only "unknown" when nothing was priced: unpriced usage is never
 * shown as $0. `short` uses "?" for charts.
 */
export function formatCost(cost: AiCost, short = false): string {
  switch (cost.kind) {
    case "estimated":
      return formatUsd(cost.usd);
    case "partial": {
      const unknown = short ? "?" : "unknown";
      return cost.knownUsd > 0
        ? `${formatUsd(cost.knownUsd)} + ${unknown}`
        : short
          ? "?"
          : "Unknown";
    }
  }
}

/** The known part of a cost: the whole estimate, or the priced subtotal. */
export function knownUsd(cost: AiCost): number {
  switch (cost.kind) {
    case "estimated":
      return cost.usd;
    case "partial":
      return cost.knownUsd;
  }
}

/** Every token across the four disjoint categories. */
export function totalTokens(tokens: AiTokenCounts): number {
  return tokens.input + tokens.cacheRead + tokens.cacheWrite + tokens.output;
}

/** A token or request count in compact form, such as `12.3K` or `4.5M`. */
export function formatCompact(count: number): string {
  return compact.format(count);
}

/** A count with thousands separators. */
export function formatCount(count: number): string {
  return whole.format(count);
}

/** A share between 0 and 1 as a percentage. */
export function formatPercent(share: number): string {
  return percent.format(share);
}

/** A model name; an empty name is a model the provider did not report. */
export function modelLabel(model: string): string {
  return model === "" ? "Unknown model" : model;
}

/** The label of a project key's usage: its project's name, or the key. */
export function attributionLabel(attribution: AiProjectAttribution): string {
  return attribution.status === "mapped"
    ? attribution.project.projectName
    : attribution.projectKey;
}

/** Splits a local `YYYY-MM-DD` date into its calendar fields. */
function dateParts(date: string): readonly [number, number, number] {
  const [year, month, day] = date.split("-").map(Number);
  return [year ?? 1970, month ?? 1, day ?? 1];
}

/**
 * The instant at UTC midnight of a local calendar date. Formatting it in UTC
 * shows the calendar date itself, whatever the browser's zone.
 */
export function calendarInstant(date: string): Date {
  const [year, month, day] = dateParts(date);
  return new Date(Date.UTC(year, month - 1, day));
}

const shortDate = new Intl.DateTimeFormat("en-GB", {
  timeZone: "UTC",
  day: "numeric",
  month: "short",
});
const longDate = new Intl.DateTimeFormat("en-GB", {
  timeZone: "UTC",
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});

/** A local calendar date as `22 Sep`. */
export function formatShortDate(date: string): string {
  return shortDate.format(calendarInstant(date));
}

/** A local calendar date as `Tue, 22 Sep 2026`. */
export function formatLongDate(date: string): string {
  return longDate.format(calendarInstant(date));
}

const HOUR_MS = 60 * 60 * 1000;

/**
 * The local hours a session was active in `timeZone` (the server's AI usage
 * zone, not the browser's): from the start of its first hour to the end of
 * its last, such as `28 Sept, 12:00–15:00`. If the clock changes inside the
 * span, UTC offsets disambiguate its endpoints. Both arguments are UTC
 * instants of hour starts.
 */
export function formatHourSpan(
  firstHour: string,
  lastHour: string,
  timeZone: string,
): string {
  const day = new Intl.DateTimeFormat("en-GB", {
    timeZone,
    day: "numeric",
    month: "short",
  });
  const time = new Intl.DateTimeFormat("en-GB", {
    timeZone,
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  });
  const offset = new Intl.DateTimeFormat("en-GB", {
    timeZone,
    timeZoneName: "shortOffset",
  });
  const start = new Date(firstHour);
  const end = new Date(new Date(lastHour).getTime() + HOUR_MS);
  const offsetAt = (instant: Date) =>
    offset.formatToParts(instant).find((part) => part.type === "timeZoneName")
      ?.value;
  const startOffset = offsetAt(start);
  const endOffset = offsetAt(end);
  const clockChanges = startOffset !== endOffset;
  const startDay = day.format(start);
  // An hour that ends at midnight still belongs to the day it started on.
  const endDay = day.format(new Date(end.getTime() - 1));
  const endTime = time.format(end);
  const startLabel = `${time.format(start)}${clockChanges ? ` ${startOffset}` : ""}`;
  const endLabel = `${startDay === endDay && endTime === "00:00" ? "24:00" : endTime}${clockChanges ? ` ${endOffset}` : ""}`;
  return startDay === endDay
    ? `${startDay}, ${startLabel}–${endLabel}`
    : `${startDay}, ${startLabel} – ${day.format(end)}, ${endLabel}`;
}

/** ISO weekday names, 1 (Monday) to 7 (Sunday). */
export const WEEKDAYS = [
  "Mon",
  "Tue",
  "Wed",
  "Thu",
  "Fri",
  "Sat",
  "Sun",
] as const;

/** A local hour of day as `14:00`. */
export function formatHour(hour: number): string {
  return `${String(hour).padStart(2, "0")}:00`;
}
