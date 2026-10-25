import type {
  AiMeteredAccountError,
  AiMeteredCheck,
  AiMeteredChargeKind,
  AiMeteredFlag,
  AiMeteredGroupKind,
  AiMeteredImport,
  AiMeteredMonth,
  AiMeteredOwner,
  AiMeteredSourceKind,
  AiMeteredStaleReason,
  AiResidualDay,
  AiResidualStatus,
} from "@/lib/api/queries/ai-admin";
import {
  formatShare as formatPercent,
  formatInstant,
  monthLabel,
  shortDate,
} from "./format";
import { monthDays } from "./coverage";

/*
 * Pure helpers for the GitHub Copilot reconciliation: GitHub's gross credits
 * (G) per account against Toki's tracked Copilot usage (T). Credits arrive as
 * exact hundredths of a credit (`12.34`, a credit is $0.01); days are UTC
 * calendar days, which compare correctly as strings.
 */

const whole = new Intl.NumberFormat("en-US");

/** Hundredths of a credit, `-12.34` as -1234. */
export function creditHundredths(credits: string): number {
  const negative = credits.startsWith("-");
  const [units, fraction = "0"] = (negative ? credits.slice(1) : credits).split(
    ".",
  );
  const magnitude = Number(units) * 100 + Number(fraction.padEnd(2, "0"));
  return negative ? -magnitude : magnitude;
}

/** Credits as a grouped number: `1,234.56`, or `-0.05`. */
export function formatCreditNumber(credits: string): string {
  const amount = creditHundredths(credits);
  const magnitude = Math.abs(amount);
  const sign = amount < 0 ? "-" : "";
  return `${sign}${whole.format(Math.floor(magnitude / 100))}.${String(magnitude % 100).padStart(2, "0")}`;
}

/** Credits with their unit: `1,234.56 credits`, `1.00 credit`. */
export function formatCredits(credits: string): string {
  return `${formatCreditNumber(credits)} ${creditHundredths(credits) === 100 ? "credit" : "credits"}`;
}

const usd = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/**
 * Credits at GitHub's list rate of $0.01 each, to the cent: hundredths of a
 * credit are hundredths of a cent, rounded half away from zero, so
 * `1234.56` credits are `$12.35`.
 */
export function creditsAsUsd(credits: string): string {
  const amount = creditHundredths(credits);
  const cents = Math.floor((Math.abs(amount) + 50) / 100);
  return usd.format((amount < 0 ? -cents : cents) / 100);
}

/** An exact USD amount from the server, `1234.50`, as `$1,234.50`. */
export function formatUsdAmount(amount: string): string {
  return usd.format(Number(amount));
}

const quantity = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 });

/**
 * One of GitHub's exact quantities (prorated seats and Actions minutes carry
 * nine decimals), grouped and rounded to at most two decimals for reading:
 * `33.451612903` as `33.45`. Show the exact string alongside, as a title.
 */
export function formatQuantity(exact: string): string {
  return quantity.format(Number(exact));
}

/** A share from 0 to 1 as a percentage; `—` without one. */
export function formatShare(share: number | null): string {
  return share === null ? "—" : formatPercent(share);
}

/** G / T with two decimals: `0.90`; `—` without tracked usage. */
export function formatRatio(ratio: number | null): string {
  return ratio === null ? "—" : ratio.toFixed(2);
}

export const FLAG_LABELS: Readonly<Record<AiMeteredFlag, string>> = {
  trackedNotBilled: "Tracked, GitHub billed nothing",
  lowRatio: "GitHub below 85% of tracked",
  stalePricing: "Old pricing basis",
  unknownModel: "Unknown model",
  unlinked: "Unlinked account",
  external: "External account",
  payerTotalMismatch: "Differs from GitHub's month total",
  importFailed: "Import failed",
};

/** What each flag means, for its tooltip. */
export const FLAG_HINTS: Readonly<Record<AiMeteredFlag, string>> = {
  trackedNotBilled:
    "Toki tracked at least 100 credits on settled day-groups GitHub billed nothing for (free models aside). Days GitHub may still post to don't count.",
  lowRatio:
    "On a settled day-group, however small, GitHub billed under 85% of what Toki tracked. A group used only through Auto sits at 90% and is not flagged.",
  stalePricing:
    "Part of Toki's tracked usage was priced on an older basis than its machine uses now; it changes once the machine reprices.",
  unknownModel:
    "A model or SKU outside the catalog, or credits at another unit or price. Kept and shown, never dropped.",
  unlinked:
    "Credits on days no disposition covers. Link the account on GitHub logins.",
  external: "Credits of an account that isn't a Toki user's: Spinit overhead.",
  payerTotalMismatch:
    "The stored days don't add up to GitHub's month total at the last import.",
  importFailed:
    "The last import couldn't read this account, which held a seat in the month.",
};

export type FlagTone = "critical" | "warning" | "neutral";

/** How loud a flag is: import problems are critical, a decided external
 * account is only noted. */
export function flagTone(flag: AiMeteredFlag): FlagTone {
  switch (flag) {
    case "importFailed":
    case "payerTotalMismatch":
      return "critical";
    case "external":
      return "neutral";
    case "trackedNotBilled":
    case "lowRatio":
    case "stalePricing":
    case "unknownModel":
    case "unlinked":
      return "warning";
  }
}

const SOURCE_LABELS: Readonly<Record<AiMeteredSourceKind, string>> = {
  accountCredits: "Account credits",
  orgCredits: "Organization credits",
  orgUsage: "Organization usage",
  summary: "Month summary",
};

export function sourceLabel(source: AiMeteredSourceKind): string {
  return SOURCE_LABELS[source];
}

/** The sources whose coverage is per day: the month summary has none. */
const DAILY_SOURCES: readonly AiMeteredSourceKind[] = [
  "accountCredits",
  "orgCredits",
  "orgUsage",
];

/** The last UTC day every daily source has imported, or null before any. */
export function importedThrough(imported: AiMeteredImport): string | null {
  const days = DAILY_SOURCES.map(
    (source) =>
      imported.sources.find((state) => state.source === source)
        ?.importedThrough ?? null,
  );
  if (days.some((day) => day === null)) {
    return null;
  }
  return days.reduce<string | null>(
    (earliest, day) =>
      earliest === null || (day !== null && day < earliest) ? day : earliest,
    null,
  );
}

export type FreshnessBadge = Readonly<{
  tone: "good" | "warning" | "neutral";
  label: string;
}>;

export function freshnessBadge(imported: AiMeteredImport): FreshnessBadge {
  switch (imported.freshness) {
    case "fresh":
      return { tone: "good", label: "Fresh" };
    case "stale":
      return { tone: "warning", label: "Stale" };
    case "never":
      return { tone: "neutral", label: "Never imported" };
    case "off":
      return { tone: "neutral", label: "Off" };
  }
}

function withoutFullStop(text: string): string {
  return text.trim().replace(/[.\s]+$/, "");
}

/**
 * Why the import is stale, as a sentence without a closing full stop:
 * instants in the browser's time zone, UTC days as `Oct 20`.
 */
export function staleReasonText(reason: AiMeteredStaleReason): string {
  switch (reason.kind) {
    case "lastRunFailed":
      return `The last import failed: ${withoutFullStop(reason.error)}`;
    case "neverSucceeded":
      return "No import has succeeded yet";
    case "lastSuccessOld":
      return `The last import succeeded at ${formatInstant(reason.at)}`;
    case "sourceNotImported":
      return `No day of ${sourceLabel(reason.source).toLowerCase()} is completely imported yet`;
    case "sourceBehind":
      return `${sourceLabel(reason.source)}: imported only through ${shortDate(reason.through)} (UTC)`;
    case "summaryNeverFetched":
      return "GitHub's month summary was never fetched";
    case "summaryOld":
      return `GitHub's month summary was last fetched at ${formatInstant(reason.at)}`;
    case "accountFailed":
      return `${reason.login} couldn't be imported for ${monthLabel(reason.month)}: ${withoutFullStop(reason.error)}`;
  }
}

/** How many accounts the errors are of: one account can fail in several
 * months. */
export function accountsWithErrors(
  errors: readonly AiMeteredAccountError[],
): number {
  return new Set(errors.map((error) => error.login)).size;
}

/** Why a month shows no GitHub figures; null when it has imported data. */
export type NoDataReason =
  "beforeBackfill" | "notStarted" | "notImported" | "sourceOff";

export function noDataReason(
  data: AiMeteredMonth,
  month: string,
): NoDataReason | null {
  if (data.months.includes(month)) {
    return null;
  }
  if (month < data.import.backfillFrom.slice(0, 7)) {
    return "beforeBackfill";
  }
  if (month > data.today.slice(0, 7)) {
    return "notStarted";
  }
  return data.import.status === "ready" ? "notImported" : "sourceOff";
}

/** Where the import stands, as a short status without a full stop. */
export function freshnessText(imported: AiMeteredImport): string {
  if (imported.status === "disabled") {
    return "GitHub isn't connected: set TOKI_GITHUB__APP_PRIVATE_KEY to import Copilot usage";
  }
  if (imported.status === "misconfigured") {
    return `GitHub is misconfigured: ${imported.message ?? "the GitHub app settings can't be used"}`;
  }
  if (imported.freshness === "never") {
    return "Nothing imported from GitHub yet";
  }
  const through = importedThrough(imported);
  return through === null
    ? "No UTC day is completely imported yet"
    : `Imported through ${shortDate(through)} (UTC)`;
}

/** The first and last day of a month, `YYYY-MM`. */
function monthBounds(month: string): Readonly<{ first: string; last: string }> {
  const days = monthDays(month);
  return {
    first: days[0] ?? `${month}-01`,
    last: days.at(-1) ?? `${month}-01`,
  };
}

/** Whether an import of `month` can start: from the backfill's month through
 * today's, both UTC. */
export function canImport(
  imported: AiMeteredImport,
  month: string,
  today: string,
): boolean {
  return (
    imported.status === "ready" &&
    month >= imported.backfillFrom.slice(0, 7) &&
    month <= today.slice(0, 7)
  );
}

export const EXTERNAL_OWNER_LABEL = "External";

/** Who an owner is: the user's name, or External (not a Toki user). */
export function ownerLabel(
  owner: AiMeteredOwner,
  names: ReadonlyMap<number, string>,
): string {
  return owner.target.kind === "external"
    ? EXTERNAL_OWNER_LABEL
    : (names.get(owner.target.userId) ?? `User ${String(owner.target.userId)}`);
}

/** An owner's days in `month`, or null when it covers the whole month so
 * far: through its last day, or through `today` in the current month. */
export function ownerDays(
  owner: AiMeteredOwner,
  month: string,
  today: string,
): string | null {
  const { first, last } = monthBounds(month);
  const end = today < last ? today : last;
  if (owner.from <= first && owner.through >= end) {
    return null;
  }
  return owner.from === owner.through
    ? shortDate(owner.from)
    : `${shortDate(owner.from)} – ${shortDate(owner.through)}`;
}

const RESIDUAL_STATUS_LABELS: Readonly<Record<AiResidualStatus, string>> = {
  settled: "Settled",
  provisional: "Still settling",
  incomplete: "Import gap",
};

export function residualStatusLabel(status: AiResidualStatus): string {
  return RESIDUAL_STATUS_LABELS[status];
}

/** The days worth a row: a residual left, or not settled yet. */
export function residualDaysToShow(
  days: readonly AiResidualDay[],
): AiResidualDay[] {
  return days.filter(
    (day) => day.status !== "settled" || creditHundredths(day.residual) !== 0,
  );
}

const GROUP_KIND_LABELS: Readonly<Record<AiMeteredGroupKind, string>> = {
  trackable: "Tracked by Toki",
  untrackable: "Untrackable (cloud agent)",
  repositoryBilled: "Billed per repository",
  unrecognized: "Unrecognized",
  trackedOnly: "Only in Toki",
};

export function groupKindLabel(kind: AiMeteredGroupKind): string {
  return GROUP_KIND_LABELS[kind];
}

const CHARGE_KIND_LABELS: Readonly<Record<AiMeteredChargeKind, string>> = {
  repositoryCredits: "Code review credits",
  accountCreditsAggregate: "IDE aggregate: all accounts, not added to accounts",
  untrackableCredits: "Cloud agent",
  compute: "Actions (not billed)",
  seats: "Seats",
  unrecognized: "Unrecognized",
};

export function chargeKindLabel(kind: AiMeteredChargeKind): string {
  return CHARGE_KIND_LABELS[kind];
}

/** Flags that ask for a look. An external account is a decision already
 * made, so it is only noted. */
export function attentionFlags(
  flags: readonly AiMeteredFlag[],
): AiMeteredFlag[] {
  return flags.filter((flag) => flag !== "external");
}

/**
 * Whether the month needs a look, which marks its tab: a stale import, a
 * ready source never imported, an account flag, an org total that differs
 * from GitHub's or couldn't be checked, or credits no account explains
 * (unknown account).
 */
export function needsAttention(month: AiMeteredMonth): boolean {
  const orgCheck = checkState(month.orgCheck);
  return (
    month.import.freshness === "stale" ||
    (month.import.status === "ready" && month.import.freshness === "never") ||
    month.accounts.some(
      (account) => attentionFlags(account.flags).length > 0,
    ) ||
    orgCheck === "differs" ||
    orgCheck === "failed" ||
    creditHundredths(month.totals.unknownAccount) !== 0
  );
}

/** Where stored days stand against GitHub's month total at the last import:
 * never compared, equal, different, or the check failed. */
export type CheckState = "unchecked" | "equal" | "differs" | "failed";

export function checkState(check: AiMeteredCheck | null): CheckState {
  if (check === null) {
    return "unchecked";
  }
  if (check.error !== null) {
    return "failed";
  }
  return check.matches === true ? "equal" : "differs";
}
