import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiBillingUsage,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { formatEstimate, formatFee } from "./format";

/*
 * Grouping and wording for billing lines. The server has already decided every
 * amount, in its own currency and converted to the billing currency (SEK);
 * this only arranges and describes them. Original amounts in different
 * currencies are summed apart and never added together; converted amounts are
 * summed as the server sums them, and a sum with a line without a rate is
 * unknown, never partial.
 */

export const UNASSIGNED_LABEL = "Unassigned";
export const OVERHEAD_LABEL = "Unallocated overhead";

export type ProjectKind = "project" | "unassigned" | "overhead";

/** The billing project a line charges: a time-tracking project, Unassigned
 * usage, or a subscription fee without usage. */
export function lineProject(line: AiBillingLine): {
  key: string;
  label: string;
  kind: ProjectKind;
} {
  if (line.unallocatedOverhead) {
    return { key: "overhead", label: OVERHEAD_LABEL, kind: "overhead" };
  }
  if (line.project) {
    return {
      key: `project:${line.project.projectId}`,
      label: line.project.projectName,
      kind: "project",
    };
  }
  return { key: "unassigned", label: UNASSIGNED_LABEL, kind: "unassigned" };
}

/** Exact decimal amount to hundredths. */
function hundredths(amount: string): number {
  const [units, cents] = amount.split(".");
  return Number(units) * 100 + Number(cents);
}

function decimal(hundredthsAmount: number): string {
  return `${Math.floor(hundredthsAmount / 100)}.${String(hundredthsAmount % 100).padStart(2, "0")}`;
}

/** What a set of lines bills: the sum of their billable amounts per
 * currency, each already rounded, so it matches the CSV export. */
export type BillableTotals = {
  /** Currency code to exact hundredths. */
  amounts: ReadonlyMap<string, number>;
  /** Unpriced API records whose cost is unknown and not in `amounts`. */
  unknownRecords: number;
  /** The converted amounts in exact hundredths of the billing currency, or
   * null when a line has no exchange rate. */
  converted: number | null;
  /** The currencies of lines without an exchange rate. */
  missingCurrencies: ReadonlySet<string>;
  /** All the lines' usage, with its unrounded estimate. */
  usage: AiBillingUsage;
};

export function billableTotals(
  lines: readonly AiBillingLine[],
): BillableTotals {
  const amounts = new Map<string, number>();
  let unknownRecords = 0;
  let converted: number | null = 0;
  const missingCurrencies = new Set<string>();
  const usage = {
    apiEquivalentUsd: 0,
    unpricedRecords: 0,
    records: 0,
    tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, total: 0 },
  };
  for (const line of lines) {
    if (line.billableAmount !== null) {
      amounts.set(
        line.billableCurrency,
        (amounts.get(line.billableCurrency) ?? 0) +
          hundredths(line.billableAmount),
      );
    }
    if (line.billingMode === "api") {
      unknownRecords += line.usage.unpricedRecords;
    }
    if (line.missingRate) {
      converted = null;
      missingCurrencies.add(line.billableCurrency);
    } else if (converted !== null && line.convertedAmount !== null) {
      converted += hundredths(line.convertedAmount);
    }
    usage.apiEquivalentUsd += line.usage.apiEquivalentUsd;
    usage.unpricedRecords += line.usage.unpricedRecords;
    usage.records += line.usage.records;
    usage.tokens.input += line.usage.tokens.input;
    usage.tokens.cacheRead += line.usage.tokens.cacheRead;
    usage.tokens.cacheWrite += line.usage.tokens.cacheWrite;
    usage.tokens.output += line.usage.tokens.output;
    usage.tokens.total += line.usage.tokens.total;
  }
  return { amounts, unknownRecords, converted, missingCurrencies, usage };
}

/** Shown where a rate is still being fetched, rather than missing. */
export const PENDING_RATE_LABEL = "Fetching rate…";

/** Whether every one of `currencies` is still being fetched. */
export function allPending(
  currencies: Iterable<string>,
  pendingRates: readonly string[],
): boolean {
  return [...currencies].every((currency) => pendingRates.includes(currency));
}

/** What a set of lines bills in the billing currency; "No rate" when a line
 * could not be converted. */
export function formatConverted(
  totals: BillableTotals,
  billingCurrency: string,
  pendingRates: readonly string[] = [],
): string {
  if (totals.converted === null) {
    return allPending(totals.missingCurrencies, pendingRates)
      ? PENDING_RATE_LABEL
      : "No rate";
  }
  if (totals.converted === 0 && totals.amounts.size === 0) {
    return totals.unknownRecords > 0 ? "Unknown" : "—";
  }
  const amount = formatFee(decimal(totals.converted), billingCurrency);
  return totals.unknownRecords > 0 ? `${amount} + unknown` : amount;
}

/** What one line bills in the billing currency. */
export function lineConverted(
  line: AiBillingLine,
  billingCurrency: string,
  pendingRates: readonly string[] = [],
): string {
  if (line.missingRate) {
    return pendingRates.includes(line.billableCurrency)
      ? PENDING_RATE_LABEL
      : "No rate";
  }
  if (line.convertedAmount === null) {
    return "Unknown";
  }
  const amount = formatFee(line.convertedAmount, billingCurrency);
  return line.billingMode === "api" && line.usage.unpricedRecords > 0
    ? `${amount} + unknown`
    : amount;
}

/** Billable amounts side by side per currency, such as `454.19 SEK + 0.75
 * USD`; never summed across currencies. */
export function formatBillable(totals: BillableTotals): string {
  const parts = [...totals.amounts.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([currency, amount]) => formatFee(decimal(amount), currency));
  if (totals.unknownRecords > 0) {
    parts.push("unknown");
  }
  return parts.length > 0 ? parts.join(" + ") : "—";
}

/** What one line bills: its fee share, or its API estimate in whole cents. */
export function lineBillable(line: AiBillingLine): string {
  if (line.billableAmount === null) {
    return "Unknown";
  }
  const amount = formatFee(line.billableAmount, line.billableCurrency);
  return line.billingMode === "api" && line.usage.unpricedRecords > 0
    ? `${amount} + unknown`
    : amount;
}

/** Why a line's figures need a second look, if they do. */
export function lineWarnings(
  line: AiBillingLine,
  subscription: AiSubscriptionMonth | undefined,
  billingCurrency = "SEK",
  pendingRates: readonly string[] = [],
): string[] {
  const warnings: string[] = [];
  if (line.missingRate && pendingRates.includes(line.billableCurrency)) {
    warnings.push(
      `The ${line.billableCurrency} exchange rate is still being fetched: not converted to ${billingCurrency} yet.`,
    );
  } else if (line.missingRate) {
    warnings.push(
      `No ${line.billableCurrency} exchange rate for the month: not converted to ${billingCurrency}.`,
    );
  }
  const unpriced = line.usage.unpricedRecords;
  if (line.unallocatedOverhead) {
    warnings.push("No usage on the days the subscription covers.");
  } else if (line.billingMode === "api" && unpriced > 0) {
    warnings.push(
      `${unpriced} unpriced records: their cost is unknown and not billed.`,
    );
  } else if (subscription?.allocation === "tokens") {
    warnings.push("Split by token share: no covered usage has a priced cost.");
  } else if (subscription?.allocation === "records") {
    warnings.push(
      "Split by record share: covered usage has no priced cost or tokens.",
    );
  }
  if (
    line.billingMode === "subscription" &&
    subscription?.allocation === "apiCost" &&
    unpriced > 0
  ) {
    warnings.push(
      `${unpriced} unpriced records carry no weight in the fee split.`,
    );
  }
  return warnings;
}

export type LineGroup = {
  key: string;
  label: string;
  kind: ProjectKind | "developer";
  /** The developer, when grouped by developer. */
  userId?: number;
  lines: AiBillingLine[];
  totals: BillableTotals;
};

/** Lines grouped by project (Unassigned, then overhead, last) or by developer,
 * each group ordered for reading. */
export function groupLines(
  lines: readonly AiBillingLine[],
  developers: readonly AiBillingDeveloper[],
  by: "project" | "developer",
): LineGroup[] {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const groups = new Map<string, LineGroup>();
  for (const line of lines) {
    const project = lineProject(line);
    const key = by === "project" ? project.key : `developer:${line.userId}`;
    const group = groups.get(key) ?? {
      key,
      label:
        by === "project"
          ? project.label
          : (names.get(line.userId) ?? `User ${line.userId}`),
      kind: by === "project" ? project.kind : "developer",
      userId: by === "developer" ? line.userId : undefined,
      lines: [],
      totals: billableTotals([]),
    };
    group.lines.push(line);
    groups.set(key, group);
  }

  const kindRank: Record<LineGroup["kind"], number> = {
    project: 0,
    developer: 0,
    unassigned: 1,
    overhead: 2,
  };
  return [...groups.values()]
    .map((group) => ({ ...group, totals: billableTotals(group.lines) }))
    .sort(
      (a, b) =>
        kindRank[a.kind] - kindRank[b.kind] || a.label.localeCompare(b.label),
    );
}

/** A month of API-equivalent cost per project, largest first, for a chart. */
export function costByProject(
  lines: readonly AiBillingLine[],
): { label: string; usd: number; display: string }[] {
  const projects = new Map<string, { label: string; lines: AiBillingLine[] }>();
  for (const line of lines) {
    if (line.unallocatedOverhead) {
      continue;
    }
    const project = lineProject(line);
    const entry = projects.get(project.key) ?? {
      label: project.label,
      lines: [],
    };
    entry.lines.push(line);
    projects.set(project.key, entry);
  }
  return [...projects.values()]
    .map(({ label, lines }) => {
      const { usage } = billableTotals(lines);
      return {
        label,
        usd: usage.apiEquivalentUsd,
        display: formatEstimate(usage),
      };
    })
    .sort((a, b) => b.usd - a.usd || a.label.localeCompare(b.label));
}

export function allocationLabel(
  allocation: AiSubscriptionMonth["allocation"],
): string {
  switch (allocation) {
    case "apiCost":
      return "By API-equivalent cost";
    case "tokens":
      return "By token share (nothing priced)";
    case "records":
      return "By record share (no cost or tokens)";
    case "unallocated":
      return "Unallocated overhead (no usage)";
  }
}
