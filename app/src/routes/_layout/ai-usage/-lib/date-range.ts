import { calendarInstant } from "./format";

/** The longest range the server reports on, in days. */
export const MAX_RANGE_DAYS = 366;

/** An inclusive range of local calendar dates, `YYYY-MM-DD`. */
export type LocalDateRange = Readonly<{ from: string; to: string }>;

const DAY_MS = 24 * 60 * 60 * 1000;

function isoDate(instant: Date): string {
  return instant.toISOString().slice(0, 10);
}

/** The date `days` after (or before, when negative) a local date. */
export function addDays(date: string, days: number): string {
  return isoDate(new Date(calendarInstant(date).getTime() + days * DAY_MS));
}

/** How many days an inclusive range covers. */
export function rangeDays(range: LocalDateRange): number {
  return (
    Math.round(
      (calendarInstant(range.to).getTime() -
        calendarInstant(range.from).getTime()) /
        DAY_MS,
    ) + 1
  );
}

/** Every date of an inclusive range, in order. */
export function eachDate(range: LocalDateRange): readonly string[] {
  const days = Math.max(0, rangeDays(range));
  return Array.from({ length: days }, (_, index) => addDays(range.from, index));
}

/** The calendar month containing a local date, offset by `months`. */
export function monthOf(date: string, months = 0): LocalDateRange {
  const start = calendarInstant(date);
  const first = new Date(
    Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + months, 1),
  );
  const last = new Date(
    Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0),
  );
  return { from: isoDate(first), to: isoDate(last) };
}

/** A named range relative to the server's `today`. */
export type RangePreset = Readonly<{ label: string; range: LocalDateRange }>;

/**
 * Presets relative to `today` in the server's AI usage time zone. Computing
 * them from the server's date keeps them right in any browser zone.
 */
export function rangePresets(today: string): readonly RangePreset[] {
  return [
    { label: "This month", range: monthOf(today) },
    { label: "Last month", range: monthOf(today, -1) },
    { label: "Last 7 days", range: { from: addDays(today, -6), to: today } },
    { label: "Last 30 days", range: { from: addDays(today, -29), to: today } },
    { label: "Last 90 days", range: { from: addDays(today, -89), to: today } },
    {
      label: "This year",
      range: { from: `${today.slice(0, 4)}-01-01`, to: today },
    },
  ];
}

/** Whether two ranges cover the same dates. */
export function sameRange(a: LocalDateRange, b: LocalDateRange): boolean {
  return a.from === b.from && a.to === b.to;
}

/** A range being picked in the calendar: no day yet, its first day, or both ends. */
export type RangeDraft =
  | Readonly<{ kind: "empty" }>
  | Readonly<{ kind: "start"; from: string }>
  | Readonly<{ kind: "complete"; range: LocalDateRange }>;

/** A draft with no day picked. */
export const EMPTY_DRAFT: RangeDraft = { kind: "empty" };

/**
 * The draft after clicking `day`: the first click starts a new range, the
 * second ends it (the earlier day is the start; clicking the same day twice
 * picks that one day), and a click after that starts over.
 */
export function pickDay(draft: RangeDraft, day: string): RangeDraft {
  switch (draft.kind) {
    case "empty":
    case "complete":
      return { kind: "start", from: day };
    case "start":
      return {
        kind: "complete",
        range:
          draft.from <= day
            ? { from: draft.from, to: day }
            : { from: day, to: draft.from },
      };
  }
}
