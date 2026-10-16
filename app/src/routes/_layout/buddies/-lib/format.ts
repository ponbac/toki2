import type { BattleRecord, BattleResult } from "@/lib/api/queries/buddy";

/** A local calendar month, `YYYY-MM`. */
export const MONTH_PATTERN = /^\d{4}-(0[1-9]|1[0-2])$/;

/**
 * A person's first name and last initial, "Anna L.", for places where a full
 * name does not fit. A single name is kept whole.
 */
export function shortPersonName(fullName: string): string {
  const parts = fullName.trim().split(/\s+/).filter(Boolean);
  const first = parts[0];
  const last = parts.at(-1);
  if (!first) {
    return "Someone";
  }
  return last && parts.length > 1 ? `${first} ${last.charAt(0)}.` : first;
}

/** Wins, losses and draws as "3–1", with the draws only when there are any: "3–1–2". */
export function formatRecord(record: BattleRecord): string {
  const { wins, losses, draws } = record;
  return draws > 0 ? `${wins}–${losses}–${draws}` : `${wins}–${losses}`;
}

/** The record in words, for tooltips and screen readers. */
export function describeRecord(record: BattleRecord): string {
  return [
    plural(record.wins, "win"),
    plural(record.losses, "loss", "losses"),
    plural(record.draws, "draw"),
  ].join(", ");
}

export function plural(
  count: number,
  noun: string,
  nouns = `${noun}s`,
): string {
  return `${count.toLocaleString("en-US")} ${count === 1 ? noun : nouns}`;
}

const RESULT_LABELS: Readonly<Record<BattleResult, string>> = {
  win: "Win",
  loss: "Loss",
  draw: "Draw",
};

/** "Win", "Loss" or "Draw". */
export function resultLabel(result: BattleResult): string {
  return RESULT_LABELS[result];
}

/** The month `delta` months after `month`, both `YYYY-MM`. */
export function shiftMonth(month: string, delta: number): string {
  const [year = 0, monthNumber = 1] = month.split("-").map(Number);
  const index = year * 12 + (monthNumber - 1) + delta;
  const shiftedYear = Math.floor(index / 12);
  const shiftedMonth = (index % 12) + 1;
  return `${shiftedYear}-${String(shiftedMonth).padStart(2, "0")}`;
}

/**
 * A month's name, "October", with the year when it is not the year of
 * `currentMonth`: "December 2025".
 */
export function monthLabel(month: string, currentMonth: string): string {
  const [year = 0, monthNumber = 1] = month.split("-").map(Number);
  const date = new Date(Date.UTC(year, monthNumber - 1, 1));
  const sameYear = month.slice(0, 4) === currentMonth.slice(0, 4);
  return new Intl.DateTimeFormat("en-US", {
    month: "long",
    year: sameYear ? undefined : "numeric",
    timeZone: "UTC",
  }).format(date);
}

/**
 * Time left before the next battle, rounded up to whole minutes: "42 min".
 * The cooldown is an hour at most, so hours only show if the clocks disagree.
 */
export function formatWait(milliseconds: number): string {
  const minutes = Math.max(1, Math.ceil(milliseconds / 60_000));
  if (minutes <= 60) {
    return `${minutes} min`;
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours} h` : `${hours} h ${rest} min`;
}
