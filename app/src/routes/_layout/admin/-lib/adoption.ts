import type {
  AiAdoption,
  AiAdoptionDay,
  AiAdoptionPerson,
} from "@/lib/api/queries/ai-admin";
import { AI_PROVIDERS, type AiProvider } from "@/lib/api/contracts/ai-usage";

/*
 * What the Adoption tab derives from the server's days. Days are local
 * calendar dates, `YYYY-MM-DD`; date arithmetic runs in UTC so it never
 * shifts a day. A person used AI on a day when any of their tools has a row
 * that day. Work days are Monday to Friday.
 */

const DAY_MS = 86_400_000;

function utc(day: string): Date {
  return new Date(`${day}T00:00:00Z`);
}

function iso(date: Date): string {
  return date.toISOString().slice(0, 10);
}

export function addDays(day: string, days: number): string {
  return iso(new Date(utc(day).getTime() + days * DAY_MS));
}

/** Monday to Friday. */
export function isWorkDay(day: string): boolean {
  const weekday = utc(day).getUTCDay();
  return weekday !== 0 && weekday !== 6;
}

/** Every day from `from` through `to`. */
export function daysOf(range: DayRange): string[] {
  const days: string[] = [];
  for (let day = range.from; day <= range.to; day = addDays(day, 1)) {
    days.push(day);
  }
  return days;
}

/** The Monday of `day`'s week. */
export function mondayOf(day: string): string {
  return addDays(day, -((utc(day).getUTCDay() + 6) % 7));
}

/** Days from `from` through `to`, both inclusive. */
export type DayRange = Readonly<{ from: string; to: string }>;

/** How many weeks back the tab looks. */
export const ADOPTION_WINDOWS = [4, 8, 12] as const;
export type AdoptionWindow = (typeof ADOPTION_WINDOWS)[number];

/** The `weeks` weeks through `today`. */
export function windowOf(today: string, weeks: number): DayRange {
  return { from: addDays(today, 1 - 7 * weeks), to: today };
}

/** The window of the same length just before `range`. */
export function previousWindow(range: DayRange): DayRange {
  const length = daysOf(range).length;
  return { from: addDays(range.from, -length), to: addDays(range.from, -1) };
}

/** Whether the server's days cover all of `range`. */
export function covers(adoption: AiAdoption, range: DayRange): boolean {
  return adoption.from <= range.from && range.to <= adoption.to;
}

/** One person's use on one day, all tools together. */
export type PersonDay = Readonly<{
  costUsd: number;
  tokens: number;
  /** Records without a known cost: `costUsd` is then a known subtotal. */
  unpricedRecords: number;
  /** Largest cost first. */
  tools: readonly AiProvider[];
}>;

/** Each person's days, by user id and day. */
export type DayIndex = ReadonlyMap<number, ReadonlyMap<string, PersonDay>>;

export function indexDays(days: readonly AiAdoptionDay[]): DayIndex {
  const rows = new Map<number, Map<string, AiAdoptionDay[]>>();
  for (const row of days) {
    const person = rows.get(row.userId) ?? new Map<string, AiAdoptionDay[]>();
    rows.set(row.userId, person);
    person.set(row.day, [...(person.get(row.day) ?? []), row]);
  }
  const index = new Map<number, Map<string, PersonDay>>();
  for (const [userId, byDay] of rows) {
    const person = new Map<string, PersonDay>();
    for (const [day, dayRows] of byDay) {
      person.set(day, {
        costUsd: sum(dayRows.map((row) => row.costUsd)),
        unpricedRecords: sum(dayRows.map((row) => row.unpricedRecords)),
        tokens: sum(dayRows.map((row) => row.tokens)),
        tools: [...dayRows]
          .sort((a, b) => b.costUsd - a.costUsd || b.tokens - a.tokens)
          .map((row) => row.tool),
      });
    }
    index.set(userId, person);
  }
  return index;
}

/** How often someone used AI in a window, by share of its work days. */
export type Habit = "daily" | "weekly" | "sometimes" | "unseen";

export function habitOf(share: number): Habit {
  if (share >= 0.75) return "daily";
  if (share >= 0.25) return "weekly";
  if (share > 0) return "sometimes";
  return "unseen";
}

/** One person's use over a window. */
export type PersonStats = Readonly<{
  person: AiAdoptionPerson;
  /** Work days of the window with any use. */
  activeWorkDays: number;
  workDays: number;
  /** `activeWorkDays / workDays`. */
  share: number;
  /** Days of the window with any use, weekends included. */
  activeDays: number;
  costUsd: number;
  tokens: number;
  /** Records without a known cost: `costUsd` is then a known subtotal. */
  unpricedRecords: number;
  /** Cost per day with use. */
  perActiveDayUsd: number;
  /** Tools used in the window, largest cost first. */
  tools: readonly AiProvider[];
  habit: Habit;
}>;

export function personStats(
  person: AiAdoptionPerson,
  days: readonly AiAdoptionDay[],
  index: DayIndex,
  range: DayRange,
): PersonStats {
  const byDay = index.get(person.userId);
  const inRange = daysOf(range);
  const workDays = inRange.filter(isWorkDay);
  const used = inRange.filter((day) => byDay?.has(day));
  const activeWorkDays = used.filter(isWorkDay).length;
  const toolCost = new Map<AiProvider, number>();
  for (const row of days) {
    if (
      row.userId === person.userId &&
      range.from <= row.day &&
      row.day <= range.to
    ) {
      toolCost.set(row.tool, (toolCost.get(row.tool) ?? 0) + row.costUsd);
    }
  }
  const personDays = used.map((day) => byDay?.get(day)).filter(isPresent);
  const costUsd = sum(personDays.map((day) => day.costUsd));
  const share = workDays.length > 0 ? activeWorkDays / workDays.length : 0;
  return {
    person,
    activeWorkDays,
    workDays: workDays.length,
    share,
    activeDays: used.length,
    costUsd,
    tokens: sum(personDays.map((day) => day.tokens)),
    unpricedRecords: sum(personDays.map((day) => day.unpricedRecords)),
    perActiveDayUsd: used.length > 0 ? costUsd / used.length : 0,
    tools: [...toolCost.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([tool]) => tool),
    habit: habitOf(share),
  };
}

/** Everyone listed, with their use over `range`: most cost first, then by
 * name. */
export function everyone(
  adoption: AiAdoption,
  index: DayIndex,
  range: DayRange,
): PersonStats[] {
  return adoption.people
    .map((person) => personStats(person, adoption.days, index, range))
    .sort(
      (a, b) =>
        b.costUsd - a.costUsd ||
        b.activeDays - a.activeDays ||
        a.person.fullName.localeCompare(b.person.fullName),
    );
}

/** The company's use over `range`: everyone's days and the credits no
 * person carries. */
export function companyTotals(
  adoption: AiAdoption,
  range: DayRange,
): Readonly<{
  costUsd: number;
  tokens: number;
  unpricedRecords: number;
  unassignedUsd: number;
}> {
  const inRange = (day: string) => range.from <= day && day <= range.to;
  const days = adoption.days.filter((row) => inRange(row.day));
  const unassignedUsd = sum(
    adoption.companyDays
      .filter((row) => inRange(row.day))
      .map((row) => row.costUsd),
  );
  return {
    costUsd: sum(days.map((row) => row.costUsd)) + unassignedUsd,
    tokens: sum(days.map((row) => row.tokens)),
    unpricedRecords: sum(days.map((row) => row.unpricedRecords)),
    unassignedUsd,
  };
}

/** The company's cost per day of `range`, credits no person carries
 * included. */
export function companyCostByDay(
  adoption: AiAdoption,
  range: DayRange,
): ReadonlyMap<string, number> {
  const costs = new Map(daysOf(range).map((day) => [day, 0]));
  for (const row of [...adoption.days, ...adoption.companyDays]) {
    const cost = costs.get(row.day);
    if (cost !== undefined) costs.set(row.day, cost + row.costUsd);
  }
  return costs;
}

/** What the volume chart measures. */
export type VolumeMeasure = "cost" | "tokens";
/** One bar per week or per calendar month. */
export type VolumePeriod = "week" | "month";

/** One bar of the volume chart: its days and each tool's part. */
export type VolumeBar = Readonly<{
  /** The bar's first day: a Monday, or a month's first day. */
  start: string;
  /** Its last day: the period's, or today for the current one. */
  end: string;
  /** The period holding today, still running through its last day. */
  partial: boolean;
  tools: Readonly<Partial<Record<AiProvider, number>>>;
  total: number;
}>;

/** How many weeks the weekly chart shows. */
export const VOLUME_WEEKS = 13;

/**
 * The volume chart's bars through `adoption.today`: the last `VOLUME_WEEKS`
 * weeks from Monday, or every calendar month since the server's range began.
 * Tokens come from people's machines only; cost includes the credits no
 * person carries, on the payer's tool.
 */
export function volumeBars(
  adoption: AiAdoption,
  measure: VolumeMeasure,
  period: VolumePeriod,
  userId?: number,
): VolumeBar[] {
  const today = adoption.today;
  const starts: string[] = [];
  if (period === "week") {
    const thisWeek = mondayOf(today);
    for (let week = VOLUME_WEEKS - 1; week >= 0; week--) {
      starts.push(addDays(thisWeek, -7 * week));
    }
  } else {
    for (
      let month = `${adoption.from.slice(0, 7)}-01`;
      month <= today;
      month = nextMonth(month)
    ) {
      starts.push(month);
    }
  }
  const bars = starts.map((start) => {
    const next = period === "week" ? addDays(start, 7) : nextMonth(start);
    const last = addDays(next, -1);
    return {
      start,
      end: last < today ? last : today,
      partial: last >= today,
      tools: {} as Partial<Record<AiProvider, number>>,
      total: 0,
    };
  });
  const barOf = (day: string) =>
    bars.find((bar) => bar.start <= day && day <= bar.end);
  const add = (day: string, tool: AiProvider, value: number) => {
    const bar = barOf(day);
    if (!bar || value === 0) return;
    bar.tools[tool] = (bar.tools[tool] ?? 0) + value;
    bar.total += value;
  };
  for (const row of adoption.days) {
    if (userId !== undefined && row.userId !== userId) continue;
    add(row.day, row.tool, measure === "cost" ? row.costUsd : row.tokens);
  }
  if (measure === "cost" && userId === undefined) {
    for (const row of adoption.companyDays) add(row.day, row.tool, row.costUsd);
  }
  return bars;
}

/** The tools with any part in `bars`, in display order. */
export function toolsIn(bars: readonly VolumeBar[]): AiProvider[] {
  return AI_PROVIDERS.filter((tool) =>
    bars.some((bar) => (bar.tools[tool] ?? 0) > 0),
  );
}

/** How many people used each tool in each of the last `VOLUME_WEEKS` weeks,
 * and in `range`. */
export function toolReach(
  adoption: AiAdoption,
  range: DayRange,
): Array<
  Readonly<{ tool: AiProvider; people: number; weekly: readonly number[] }>
> {
  const thisWeek = mondayOf(adoption.today);
  const weeks = Array.from({ length: VOLUME_WEEKS }, (_, index) =>
    addDays(thisWeek, -7 * (VOLUME_WEEKS - 1 - index)),
  );
  return AI_PROVIDERS.map((tool) => {
    const rows = adoption.days.filter((row) => row.tool === tool);
    const peopleIn = (from: string, to: string) =>
      new Set(
        rows
          .filter((row) => from <= row.day && row.day <= to)
          .map((row) => row.userId),
      ).size;
    return {
      tool,
      people: peopleIn(range.from, range.to),
      weekly: weeks.map((week) => peopleIn(week, addDays(week, 6))),
    };
  }).filter((reach) => reach.people > 0 || reach.weekly.some((n) => n > 0));
}

/** Cost steps of a day's squares, in USD: under the first is the lightest. */
export const VOLUME_STEPS_USD = [1, 5, 25, 100] as const;

/** A day's step on the sequential ramp, 0 (least) to 4 (most). */
export function volumeStep(costUsd: number): number {
  return VOLUME_STEPS_USD.filter((step) => costUsd >= step).length;
}

export function median(values: readonly number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1
    ? sorted[middle]
    : (sorted[middle - 1] + sorted[middle]) / 2;
}

/** The share of `total` the `count` largest values make up. */
export function topShare(
  values: readonly number[],
  count: number,
  total: number,
): number {
  if (total <= 0) return 0;
  return sum([...values].sort((a, b) => b - a).slice(0, count)) / total;
}

function nextMonth(firstDay: string): string {
  const [year, month] = firstDay.split("-").map(Number);
  return month === 12
    ? `${year + 1}-01-01`
    : `${year}-${String(month + 1).padStart(2, "0")}-01`;
}

function sum(values: readonly number[]): number {
  return values.reduce((total, value) => total + value, 0);
}

function isPresent<T>(value: T | undefined): value is T {
  return value !== undefined;
}
