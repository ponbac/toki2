import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiBillingSubject,
  AiBillingUsage,
  AiMeteredProvisionalReason,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { formatFee } from "@/lib/ai-subscriptions";
import type { UnassignedKind } from "@/lib/api/contracts/ai-admin";
import type { AiProvider } from "@/lib/api/contracts/ai-usage";

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
/** Overhead where its label does not fit, as in a narrow strip segment. */
export const OVERHEAD_SHORT_LABEL = "Overhead";

/** Each kind of Unassigned usage: its name, the name in a word or two where
 * that does not fit, and what it is. */
export const UNASSIGNED_KIND_LABELS = {
  unmapped: {
    label: "Unmapped repositories",
    short: "Unmapped",
    detail: "unmapped repositories",
  },
  unattributed: {
    label: "No repository",
    short: "No repo",
    detail: "outside any repository",
  },
} as const satisfies Record<UnassignedKind, unknown>;

/** A metered payer's credits that stay with Spinit, by why, as the ledger
 * groups them: credits Toki did not track (the cloud agent's included),
 * personal and internal usage, and credits of accounts no developer owns:
 * unlinked, external or unknown. */
export const SPINIT_KINDS = ["untracked", "personal", "accounts"] as const;
/** The ledger's kinds of Spinit's credits, and the statement's own: a
 * metered payer's credits outside any repository (`unattributed`), which the
 * ledger keeps under Unassigned and the statement counts as Spinit's. */
export type SpinitKind = (typeof SPINIT_KINDS)[number] | "unattributed";

/** Each kind of Spinit's credits: its name, the name in a word where that does
 * not fit, and what it is after "Spinit". */
export const SPINIT_KIND_LABELS = {
  untracked: {
    label: "Spinit: untracked Copilot",
    short: "Untracked",
    detail: "untracked Copilot",
  },
  personal: {
    label: "Spinit: personal / internal Copilot",
    short: "Personal",
    detail: "personal / internal Copilot",
  },
  accounts: {
    label: "Spinit: unlinked, external and unknown accounts",
    short: "Accounts",
    detail: "unlinked, external, unknown accounts",
  },
  unattributed: {
    label: "Spinit: Copilot outside any repository",
    short: "Outside repos",
    detail: "Copilot outside any repository",
  },
} as const satisfies Record<SpinitKind, unknown>;

/** Spinit, as the start of its entries' names. */
export const SPINIT_LABEL = "Spinit";

export type ProjectKind = "project" | "unassigned" | "overhead" | "spinit";

/** What a metered payer's lines are called: its billing system's name. */
export function meteredName(provider: AiProvider): string {
  return provider === "copilot"
    ? "GitHub Copilot"
    : AI_PROVIDER_LABELS[provider];
}

/** The billing project a line charges: a time-tracking project, one kind of
 * Unassigned usage, a subscription fee without usage, or one kind of a
 * metered payer's credits that stay with Spinit. */
export function lineProject(line: AiBillingLine): {
  key: string;
  label: string;
  kind: ProjectKind;
  /** The kind of Unassigned usage; null for every other kind. */
  unassigned: UnassignedKind | null;
  /** The kind of Spinit's credits; null for every other kind. */
  spinit: SpinitKind | null;
} {
  const spinit = spinitKind(line);
  if (spinit !== null) {
    return {
      key: `spinit:${spinit}`,
      label: SPINIT_KIND_LABELS[spinit].label,
      kind: "spinit",
      unassigned: null,
      spinit,
    };
  }
  if (line.unallocatedOverhead) {
    return {
      key: "overhead",
      label: OVERHEAD_LABEL,
      kind: "overhead",
      unassigned: null,
      spinit: null,
    };
  }
  if (line.project) {
    return {
      key: `project:${line.project.projectId}`,
      label: line.project.projectName,
      kind: "project",
      unassigned: null,
      spinit: null,
    };
  }
  // The contract sets a kind on every line without a project that bills one.
  const unassigned = line.unassigned ?? "unmapped";
  return {
    key: `unassigned:${unassigned}`,
    label: UNASSIGNED_KIND_LABELS[unassigned].label,
    kind: "unassigned",
    unassigned,
    spinit: null,
  };
}

/** Which of Spinit's credits a metered line is; null for a line that bills a
 * project or Unassigned, and for every other line. */
function spinitKind(line: AiBillingLine): SpinitKind | null {
  switch (line.metered?.kind) {
    case "untracked":
      return "untracked";
    case "personal":
      return "personal";
    case "unlinkedAccount":
    case "externalAccount":
    case "unknownAccount":
      return "accounts";
    case "project":
    case "repository":
    case undefined:
      return null;
  }
}

/** A line's developer; null for an account, the unknown account or a
 * repository. */
export function lineUserId(line: AiBillingLine): number | null {
  return line.subject.kind === "user" ? line.subject.userId : null;
}

/** A subject as a key, unique across kinds. */
export function subjectKey(subject: AiBillingSubject): string {
  switch (subject.kind) {
    case "user":
      return `user:${subject.userId}`;
    case "providerAccount":
      return `account:${subject.provider}:${subject.accountId}`;
    case "unknownAccount":
      return `unknown:${subject.provider}`;
    case "repository":
      return `repository:${subject.provider}:${subject.name}`;
  }
}

/** Unknown accounts, as a subject is called. */
export const UNKNOWN_ACCOUNTS_LABEL = "Unknown accounts";

/** What a subject is called: a developer by name, an account by login, a
 * repository by name. */
export function subjectLabel(
  subject: AiBillingSubject,
  names: ReadonlyMap<number, string>,
): string {
  switch (subject.kind) {
    case "user":
      return names.get(subject.userId) ?? `User ${subject.userId}`;
    case "providerAccount":
      return subject.login;
    case "unknownAccount":
      return UNKNOWN_ACCOUNTS_LABEL;
    case "repository":
      return subject.name;
  }
}

/** Why a metered line is Spinit's because of its account, in words. */
export function meteredAccountDetail(line: AiBillingLine): string | null {
  switch (line.metered?.kind) {
    case "unlinkedAccount":
      return "not linked";
    case "externalAccount":
      return "external";
    case "unknownAccount":
      return "accounts Toki never listed";
    default:
      return null;
  }
}

/** A metered line's credits, whole as the server bills them: `1,235
 * credits`, `1 credit`. */
export function formatLineCredits(credits: string): string {
  const amount = hundredths(credits);
  const whole = Math.floor(amount / 100);
  const rest = amount % 100;
  const number =
    rest === 0
      ? WHOLE.format(whole)
      : `${WHOLE.format(whole)}.${String(rest).padStart(2, "0")}`;
  return `${number} ${amount === 100 ? "credit" : "credits"}`;
}

const WHOLE = new Intl.NumberFormat("en-US");

/** A metered payer's USD amount as its credits, a credit being $0.01:
 * `1235.00` USD as `123,500 credits`. */
export function usdAsCredits(usd: string): string {
  const credits = hundredths(usd);
  return `${WHOLE.format(credits)} ${credits === 1 ? "credit" : "credits"}`;
}

/** The SKUs of credits Toki tracks: those its project and personal lines
 * bill, every one of which is on such a SKU. */
export function trackedSkus(lines: readonly AiBillingLine[]): Set<string> {
  return new Set(
    lines.flatMap((line) =>
      line.metered?.sku &&
      (line.metered.kind === "project" || line.metered.kind === "personal")
        ? [line.metered.sku]
        : [],
    ),
  );
}

/** The SKU a metered line names: one other than the tracked SKUs, such as
 * the cloud agent's; null for a line on a tracked SKU and for every other
 * line. */
export function meteredSku(
  line: AiBillingLine,
  tracked: ReadonlySet<string>,
): string | null {
  const sku = line.metered?.sku ?? null;
  return sku === null || tracked.has(sku) ? null : sku;
}

/** A metered line in words: its payer and credits, and its SKU when it is
 * not a tracked one: `GitHub Copilot · 1,235 credits`. */
export function meteredLineLabel(
  line: AiBillingLine,
  tracked: ReadonlySet<string>,
): string | null {
  if (line.metered === null) {
    return null;
  }
  const sku = meteredSku(line, tracked);
  return [
    meteredName(line.provider),
    formatLineCredits(line.metered.credits),
    ...(sku === null ? [] : [sku]),
  ].join(" · ");
}

/** Why a metered line may still change, as a sentence; null for a settled
 * line and for every other line. */
export function provisionalWarning(line: AiBillingLine): string | null {
  return line.metered?.provisional
    ? `Provisional: ${line.metered.provisionalReasons.map((reason) => PROVISIONAL_REASON_LABELS[reason]).join("; ")}.`
    : null;
}

/** Why a metered line may still change, as a clause. */
export const PROVISIONAL_REASON_LABELS: Readonly<
  Record<AiMeteredProvisionalReason, string>
> = {
  monthInProgress: "the UTC month is in progress",
  importIncomplete: "GitHub's usage is not imported for every day",
  notSettled: "some days may still change on GitHub",
  oldPricingBasis: "some tracked usage is on an old pricing basis",
  provisionalModel: "a model's rates are provisional",
  unrecognized: "an unrecognized model or credits row",
};

/** The lines no project is charged for, of one kind or of both. */
export function unassignedLines(
  lines: readonly AiBillingLine[],
  kind?: UnassignedKind,
): AiBillingLine[] {
  return lines.filter((line) => {
    const project = lineProject(line);
    return (
      project.kind === "unassigned" &&
      (kind === undefined || project.unassigned === kind)
    );
  });
}

/** Exact decimal amount to hundredths. */
export function hundredths(amount: string): number {
  const [units, cents] = amount.split(".");
  return Number(units) * 100 + Number(cents);
}

function decimal(hundredthsAmount: number): string {
  return `${Math.floor(hundredthsAmount / 100)}.${String(hundredthsAmount % 100).padStart(2, "0")}`;
}

/** Exact hundredths as an amount in `currency`, such as `6 454,93 SEK`. */
export function formatHundredths(amount: number, currency: string): string {
  return formatFee(decimal(amount), currency);
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

/** An estimate or a sum in the billing currency, to hundredths. */
export function formatAmount(amount: number, currency: string): string {
  return formatFee(amount.toFixed(2), currency);
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

/** Billable amounts apart per currency, with `unknown` last for unpriced
 * usage; never summed across currencies. Empty when nothing bills. */
export function billableParts(totals: BillableTotals): string[] {
  const parts = [...totals.amounts.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([currency, amount]) => formatFee(decimal(amount), currency));
  if (totals.unknownRecords > 0) {
    parts.push("unknown");
  }
  return parts;
}

/** Billable amounts side by side per currency, such as `454,19 SEK + 0.75
 * USD`. */
export function formatBillable(totals: BillableTotals): string {
  const parts = billableParts(totals);
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
  if (line.metered !== null) {
    const { metered } = line;
    const provisional = provisionalWarning(line);
    if (provisional !== null) {
      warnings.push(provisional);
    }
    if (metered.lowRatio) {
      warnings.push(
        "GitHub billed under 85% of what Toki tracked on a settled day.",
      );
    }
    if (unpriced > 0) {
      warnings.push(
        `${unpriced} unpriced records carry no weight in the split.`,
      );
    }
    if (metered.kind === "unknownAccount") {
      warnings.push("Code review is billed by repository and left out.");
    }
    return warnings;
  }
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
  /** A project's kind, or by subject a developer or an account (any other
   * subject). */
  kind: ProjectKind | "developer" | "account";
  /** The developer, when grouped by a developer. */
  userId?: number;
  lines: AiBillingLine[];
  totals: BillableTotals;
};

/** Lines grouped by project (Unassigned, overhead and Spinit's, last) or by
 * subject (developers, then accounts), each group ordered for reading. */
export function groupLines(
  lines: readonly AiBillingLine[],
  developers: readonly AiBillingDeveloper[],
  by: "project" | "developer",
): LineGroup[] {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const groups = new Map<string, LineGroup>();
  for (const line of lines) {
    const project = lineProject(line);
    const userId = lineUserId(line);
    const key =
      by === "project" ? project.key : `subject:${subjectKey(line.subject)}`;
    const group = groups.get(key) ?? {
      key,
      label:
        by === "project" ? project.label : subjectLabel(line.subject, names),
      kind:
        by === "project"
          ? project.kind
          : userId === null
            ? "account"
            : "developer",
      userId: by === "developer" ? (userId ?? undefined) : undefined,
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
    account: 1,
    overhead: 2,
    spinit: 3,
  };
  return [...groups.values()]
    .map((group) => ({ ...group, totals: billableTotals(group.lines) }))
    .sort(
      (a, b) =>
        kindRank[a.kind] - kindRank[b.kind] || a.label.localeCompare(b.label),
    );
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
