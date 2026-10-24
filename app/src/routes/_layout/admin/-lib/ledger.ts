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

/** One segment of the bill's allocation strip. */
export type AllocationSegment = Readonly<{
  key: string;
  kind: ProjectKind | "others";
  label: string;
  /** Exact hundredths of the billing currency; always above zero. */
  amount: number;
  /** The entries the segment stands for: one, or the small projects merged. */
  entries: readonly LedgerEntry[];
}>;

/** Projects below this share of the bill are merged into one segment. */
const SMALL_SHARE = 0.015;

/**
 * The ledger as the strip draws it: projects dearest first, the small ones
 * merged when there are two or more, then the unassigned usage and the
 * overhead. Entries without a known amount, or of nothing, take no space.
 */
export function allocationSegments(ledger: Ledger): AllocationSegment[] {
  const known = (e: LedgerEntry) =>
    e.totals.converted !== null && e.totals.converted > 0;
  const total = ledgerEntries(ledger)
    .filter(known)
    .reduce((sum, e) => sum + (e.totals.converted ?? 0), 0);
  const single = (e: LedgerEntry): AllocationSegment => ({
    key: e.key,
    kind: e.kind,
    label: e.label,
    amount: e.totals.converted ?? 0,
    entries: [e],
  });

  const projects = ledger.projects.filter(known);
  const small = projects.filter(
    (e) => (e.totals.converted ?? 0) / total < SMALL_SHARE,
  );
  const segments =
    small.length >= 2
      ? [
          ...projects.filter((e) => !small.includes(e)).map(single),
          {
            key: "others",
            kind: "others" as const,
            label: `${small.length} smaller projects`,
            amount: small.reduce(
              (sum, e) => sum + (e.totals.converted ?? 0),
              0,
            ),
            entries: small,
          },
        ]
      : projects.map(single);
  for (const e of [ledger.unassigned, ledger.overhead]) {
    if (e && known(e)) {
      segments.push(single(e));
    }
  }
  return segments;
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
