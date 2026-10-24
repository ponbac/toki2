import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiBillingOverview,
  AiBillingUsage,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { AI_PROVIDERS, type AiProvider } from "@/lib/api/contracts/ai-usage";
import {
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

/** Splits `Lerums Djursjukhus - Apport` at its first ` - `. A name without
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

/** Whoever and whatever a set of lines bills. */
export type LedgerEntry = Readonly<{
  key: string;
  kind: ProjectKind;
  label: string;
  name: ProjectName;
  /** Dearest first. */
  lines: readonly AiBillingLine[];
  totals: BillableTotals;
  /** The developers whose usage it is, dearest first. */
  developers: readonly number[];
  /** The tools used, in the providers' display order. */
  providers: readonly AiProvider[];
}>;

function entry(
  key: string,
  kind: ProjectKind,
  label: string,
  lines: readonly AiBillingLine[],
): LedgerEntry {
  const sorted = [...lines].sort((a, b) =>
    byAmount(lineHundredths(a), lineHundredths(b)),
  );
  const developers = new Map<number, number>();
  for (const line of sorted) {
    developers.set(
      line.userId,
      (developers.get(line.userId) ?? 0) + (lineHundredths(line) ?? 0),
    );
  }
  const used = new Set(
    lines.filter((line) => !line.unallocatedOverhead).map((l) => l.provider),
  );
  return {
    key,
    kind,
    label,
    name:
      kind === "project"
        ? splitProjectName(label)
        : { customer: label, project: null },
    lines: sorted,
    totals: billableTotals(sorted),
    developers: [...developers.entries()]
      .sort(([, a], [, b]) => b - a)
      .map(([userId]) => userId),
    providers: AI_PROVIDERS.filter((provider) => used.has(provider)),
  };
}

/** A month's bill by where its money goes. */
export type Ledger = Readonly<{
  /** Time-tracking projects, dearest first. */
  projects: readonly LedgerEntry[];
  /** Usage no project is charged for; null when every line has one. */
  unassigned: LedgerEntry | null;
  /** Subscription fees nobody used; null without any. */
  overhead: LedgerEntry | null;
  /** Every line together. */
  totals: BillableTotals;
}>;

/** Groups a month's lines by the project they charge, dearest first. */
export function ledgerOf(lines: readonly AiBillingLine[]): Ledger {
  const groups = new Map<
    string,
    { kind: ProjectKind; label: string; lines: AiBillingLine[] }
  >();
  for (const line of lines) {
    const project = lineProject(line);
    const group = groups.get(project.key) ?? {
      kind: project.kind,
      label: project.label,
      lines: [],
    };
    group.lines.push(line);
    groups.set(project.key, group);
  }
  const entries = [...groups.entries()].map(([key, group]) =>
    entry(key, group.kind, group.label, group.lines),
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
    unassigned: entries.find((e) => e.kind === "unassigned") ?? null,
    overhead: entries.find((e) => e.kind === "overhead") ?? null,
    totals: billableTotals(lines),
  };
}

/** Every entry of a ledger, in the order it is read: projects, then the
 * unassigned usage, then the overhead. */
export function ledgerEntries(ledger: Ledger): readonly LedgerEntry[] {
  return [
    ...ledger.projects,
    ...(ledger.unassigned ? [ledger.unassigned] : []),
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
 * space.
 */
export function allocationSegments(ledger: Ledger): AllocationSegment[] {
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
    segments.push({
      key: "others",
      kind: "others",
      label: tail.every(small)
        ? `${tail.length} smaller projects`
        : `${first.name.customer} + ${rest.length} more`,
      amount: tail.reduce((sum, e) => sum + amount(e), 0),
      entries: tail,
      step: TAIL_STEP,
    });
  }
  for (const e of [ledger.unassigned, ledger.overhead]) {
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
      hasUnassigned: ledger.unassigned !== null,
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
