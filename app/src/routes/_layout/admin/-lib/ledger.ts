import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiBillingOverview,
  AiBillingUsage,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { AI_PROVIDERS, type AiProvider } from "@/lib/api/contracts/ai-usage";
import {
  UNASSIGNED_KINDS,
  type UnassignedKind,
} from "@/lib/api/contracts/ai-admin";
import {
  OVERHEAD_SHORT_LABEL,
  UNASSIGNED_KIND_LABELS,
  billableTotals,
  hundredths,
  lineProject,
  type BillableTotals,
  type ProjectKind,
} from "./billing";

/*
 * A month's bill arranged as a ledger: where its money goes, project by
 * project, and whose usage it is, developer by developer. The server has
 * decided every amount; this only groups, ranks and measures them. Amounts
 * are compared in exact hundredths of the billing currency, and an amount
 * without a rate takes no part in a share.
 */

/** A project's name read as `Customer - Project`, the time tracker's naming. */
export type ProjectName = Readonly<{
  customer: string;
  /** What follows the customer; null for a name without the separator. */
  project: string | null;
}>;

const NAME_SEPARATOR = " - ";

/** Splits `Norrsken Logistik - Kundportal` at its first ` - `. A name without
 * the separator, or with nothing on one side of it, is all customer. */
export function splitProjectName(name: string): ProjectName {
  const at = name.indexOf(NAME_SEPARATOR);
  const customer = name.slice(0, at).trim();
  const project = name.slice(at + NAME_SEPARATOR.length).trim();
  return at < 0 || customer === "" || project === ""
    ? { customer: name, project: null }
    : { customer, project };
}

/** A line's amount in exact hundredths of the billing currency, or null
 * while it has no rate or no known cost. */
export function lineHundredths(line: AiBillingLine): number | null {
  return line.missingRate || line.convertedAmount === null
    ? null
    : hundredths(line.convertedAmount);
}

/** Orders amounts dearest first; an unknown amount after every known one. */
function byAmount(a: number | null, b: number | null): number {
  return (b ?? -1) - (a ?? -1);
}

/** The DOM id of an entry's ledger row, to scroll to it. */
export function ledgerRowId(key: string): string {
  return `ledger-${key.replace(/[^a-zA-Z0-9-]/g, "-")}`;
}

/** One developer's lines within an entry. */
export type LedgerPerson = Readonly<{
  userId: number;
  /** Dearest first. */
  lines: readonly AiBillingLine[];
  totals: BillableTotals;
  /** The tools used, in the providers' display order. */
  providers: readonly AiProvider[];
}>;

/** The tools a set of lines used, without fees nobody used. */
function providersOf(lines: readonly AiBillingLine[]): AiProvider[] {
  const used = new Set(
    lines.filter((line) => !line.unallocatedOverhead).map((l) => l.provider),
  );
  return AI_PROVIDERS.filter((provider) => used.has(provider));
}

/** Whoever and whatever a set of lines bills. */
export type LedgerEntry = Readonly<{
  key: string;
  kind: ProjectKind;
  /** The kind of Unassigned usage; null for a project and for overhead. */
  unassigned: UnassignedKind | null;
  label: string;
  name: ProjectName;
  /** Dearest first. */
  lines: readonly AiBillingLine[];
  totals: BillableTotals;
  /** Each developer's lines, dearest developer first. */
  people: readonly LedgerPerson[];
  /** The tools used, in the providers' display order. */
  providers: readonly AiProvider[];
}>;

function entry(
  key: string,
  { kind, unassigned, label }: ReturnType<typeof lineProject>,
  lines: readonly AiBillingLine[],
): LedgerEntry {
  const sorted = [...lines].sort((a, b) =>
    byAmount(lineHundredths(a), lineHundredths(b)),
  );
  const byUser = new Map<number, AiBillingLine[]>();
  for (const line of sorted) {
    byUser.set(line.userId, [...(byUser.get(line.userId) ?? []), line]);
  }
  const people = [...byUser.entries()]
    .map(([userId, own]): LedgerPerson => ({
      userId,
      lines: own,
      totals: billableTotals(own),
      providers: providersOf(own),
    }))
    .sort(
      (a, b) =>
        byAmount(a.totals.converted, b.totals.converted) ||
        b.totals.usage.apiEquivalentUsd - a.totals.usage.apiEquivalentUsd ||
        a.userId - b.userId,
    );
  return {
    key,
    kind,
    unassigned,
    label,
    name:
      kind === "project"
        ? splitProjectName(label)
        : { customer: label, project: null },
    lines: sorted,
    totals: billableTotals(sorted),
    people,
    providers: providersOf(lines),
  };
}

/** A month's bill by where its money goes. */
export type Ledger = Readonly<{
  /** Time-tracking projects, dearest first. */
  projects: readonly LedgerEntry[];
  /** Usage no project is charged for, by kind: repositories nobody mapped,
   * then usage outside any repository. Empty when every line has a project. */
  unassigned: readonly LedgerEntry[];
  /** Subscription fees nobody used; null without any. */
  overhead: LedgerEntry | null;
  /** Every line together. */
  totals: BillableTotals;
}>;

/** Groups a month's lines by the project they charge, dearest first. */
export function ledgerOf(lines: readonly AiBillingLine[]): Ledger {
  const groups = new Map<
    string,
    { project: ReturnType<typeof lineProject>; lines: AiBillingLine[] }
  >();
  for (const line of lines) {
    const project = lineProject(line);
    const group = groups.get(project.key) ?? { project, lines: [] };
    group.lines.push(line);
    groups.set(project.key, group);
  }
  const entries = [...groups.entries()].map(([key, group]) =>
    entry(key, group.project, group.lines),
  );
  return {
    projects: entries
      .filter((e) => e.kind === "project")
      .sort(
        (a, b) =>
          byAmount(a.totals.converted, b.totals.converted) ||
          b.totals.usage.apiEquivalentUsd - a.totals.usage.apiEquivalentUsd ||
          a.label.localeCompare(b.label),
      ),
    unassigned: UNASSIGNED_KINDS.flatMap(
      (kind) => entries.find((e) => e.unassigned === kind) ?? [],
    ),
    overhead: entries.find((e) => e.kind === "overhead") ?? null,
    totals: billableTotals(lines),
  };
}

/** Every entry of a ledger, in the order it is read: projects, then the
 * unassigned usage, then the overhead. */
export function ledgerEntries(ledger: Ledger): readonly LedgerEntry[] {
  return [
    ...ledger.projects,
    ...ledger.unassigned,
    ...(ledger.overhead ? [ledger.overhead] : []),
  ];
}

/** An entry's amount as a share of the ledger's, from 0 to 1; null when
 * either is unknown or the ledger bills nothing. */
export function shareOf(entry: LedgerEntry, ledger: Ledger): number | null {
  const whole = ledger.totals.converted;
  const part = entry.totals.converted;
  return whole === null || part === null || whole === 0 ? null : part / whole;
}

/**
 * A step of the billable ramp, `--billable-1` to `--billable-5`: a position
 * code that repeats the strip's dearest-first order. Step 1 is the dearest
 * project and step 5 is always the tail of small or merged projects.
 */
export type BillableStep = 1 | 2 | 3 | 4 | 5;

/** Projects at most drawn as segments of their own; the rest form the tail. */
export const OWN_SEGMENTS = 4;

/** The step of the tail, however few segments come before it. */
export const TAIL_STEP = 5;

/** One segment of the bill's allocation strip. */
export type AllocationSegment = Readonly<{
  key: string;
  kind: ProjectKind | "others";
  label: string;
  /** What the strip calls it where its label does not fit: a project by its
   * customer, the tail by its dearest customer or in a word, the rest in a
   * word or two. */
  short: string;
  /** The projects a tail named after its dearest customer merges besides
   * it, kept beside `short` however narrow; 0 for every other segment. */
  more: number;
  /** Exact hundredths of the billing currency; always above zero. */
  amount: number;
  /** The entries the segment stands for: one, or the tail merged. */
  entries: readonly LedgerEntry[];
  /** Its step of the billable ramp; null for Unassigned and overhead. */
  step: BillableStep | null;
}>;

/** Projects below this share of the bill never get a segment of their own. */
const SMALL_SHARE = 0.015;

const OWN_STEPS: readonly BillableStep[] = [1, 2, 3, 4];

/**
 * The ledger as the strip draws it. The dearest projects, up to
 * `OWN_SEGMENTS` of them and each at least `SMALL_SHARE` of the bill, get
 * segments of their own on steps 1 to 4. Every other project forms the tail
 * on step 5: kept as its own segment when it is alone, merged into one
 * `others` segment when there are more. The tail is named "N smaller
 * projects" when all of it is small, and after its dearest customer when the
 * cap merged a project that is not. Unassigned usage and overhead follow
 * without a step. Entries without a known amount, or of nothing, take no
 * space. While a line has no exchange rate the bill's total is unknown, and
 * shares of the rest would read as shares of the whole: there is no strip.
 */
export function allocationSegments(ledger: Ledger): AllocationSegment[] {
  if (ledger.totals.converted === null) {
    return [];
  }
  const known = (e: LedgerEntry) =>
    e.totals.converted !== null && e.totals.converted > 0;
  const amount = (e: LedgerEntry) => e.totals.converted ?? 0;
  const total = ledgerEntries(ledger)
    .filter(known)
    .reduce((sum, e) => sum + amount(e), 0);
  const single = (
    e: LedgerEntry,
    step: BillableStep | null,
  ): AllocationSegment => ({
    key: e.key,
    kind: e.kind,
    label: e.label,
    short: shortName(e),
    more: 0,
    amount: amount(e),
    entries: [e],
    step,
  });

  const projects = ledger.projects.filter(known);
  const small = (e: LedgerEntry) => amount(e) / total < SMALL_SHARE;
  // Projects are dearest first, so the ones with segments of their own are
  // always the top of the list.
  let ownCount = 0;
  while (
    ownCount < OWN_SEGMENTS &&
    ownCount < projects.length &&
    !small(projects[ownCount])
  ) {
    ownCount += 1;
  }
  const own = projects.slice(0, ownCount);
  const tail = projects.slice(ownCount);

  const segments = own.map((e, index) => single(e, OWN_STEPS[index]));
  const [first, ...rest] = tail;
  if (first && rest.length === 0) {
    segments.push(single(first, TAIL_STEP));
  } else if (first) {
    const named = !tail.every(small);
    segments.push({
      key: "others",
      kind: "others",
      label: named
        ? `${first.name.customer} + ${rest.length} more`
        : `${tail.length} smaller projects`,
      short: named ? first.name.customer : `${tail.length} smaller`,
      more: named ? rest.length : 0,
      amount: tail.reduce((sum, e) => sum + amount(e), 0),
      entries: tail,
      step: TAIL_STEP,
    });
  }
  for (const e of [...ledger.unassigned, ledger.overhead]) {
    if (e && known(e)) {
      segments.push(single(e, null));
    }
  }
  return segments;
}

/**
 * Each ledger entry's step, as its strip segment has it: the entries of the
 * tail all map to the tail's step. Unassigned usage, overhead and projects
 * without a segment are absent, so a row can never disagree with its segment.
 */
export function segmentSteps(
  segments: readonly AllocationSegment[],
): ReadonlyMap<string, BillableStep> {
  const steps = new Map<string, BillableStep>();
  for (const segment of segments) {
    if (segment.step !== null) {
      for (const entry of segment.entries) {
        steps.set(entry.key, segment.step);
      }
    }
  }
  return steps;
}

/** An entry's segment where its label does not fit. */
function shortName(e: LedgerEntry): string {
  switch (e.kind) {
    case "project":
      return e.name.customer;
    case "unassigned":
      return UNASSIGNED_KIND_LABELS[e.unassigned ?? "unmapped"].short;
    case "overhead":
      return OVERHEAD_SHORT_LABEL;
  }
}

/** One developer's month on the bill. */
export type PersonRow = Readonly<{
  userId: number;
  name: string;
  /** What the developer's lines bill; null when they have none. */
  totals: BillableTotals | null;
  /** The developer's subscriptions that cover days of the month. */
  plans: readonly AiSubscriptionMonth[];
  /** Usage left out as not client work; null without any. */
  personal: AiBillingUsage | null;
  /** The projects their usage bills, dearest first, without Unassigned. */
  projects: readonly string[];
  /** Whether some of their usage has no project. */
  hasUnassigned: boolean;
}>;

/**
 * Everyone on the month's bill, and everyone else named in `others` (such as
 * developers whose machines synced but billed nothing), dearest first.
 */
export function peopleOf(
  overview: AiBillingOverview,
  others: readonly AiBillingDeveloper[] = [],
): PersonRow[] {
  const names = new Map<number, string>();
  for (const developer of [...others, ...overview.developers]) {
    names.set(developer.userId, developer.fullName);
  }
  const ids = new Set<number>([
    ...overview.lines.map((line) => line.userId),
    ...overview.subscriptions.map((s) => s.userId),
    ...overview.personalUsage.map((p) => p.userId),
    ...others.map((developer) => developer.userId),
  ]);
  const rows = [...ids].map((userId): PersonRow => {
    const lines = overview.lines.filter((line) => line.userId === userId);
    const ledger = ledgerOf(lines);
    return {
      userId,
      name: names.get(userId) ?? `User ${userId}`,
      totals: lines.length > 0 ? ledger.totals : null,
      plans: overview.subscriptions.filter((s) => s.userId === userId),
      personal:
        overview.personalUsage.find((p) => p.userId === userId)?.usage ?? null,
      projects: ledger.projects.map((e) => e.label),
      hasUnassigned: ledger.unassigned.length > 0,
    };
  });
  return rows.sort(
    (a, b) =>
      byAmount(a.totals?.converted ?? null, b.totals?.converted ?? null) ||
      (b.totals?.usage.apiEquivalentUsd ?? 0) -
        (a.totals?.usage.apiEquivalentUsd ?? 0) ||
      a.name.localeCompare(b.name),
  );
}
