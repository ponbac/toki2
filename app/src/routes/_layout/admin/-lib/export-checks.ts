import type {
  AiBillingCompleteness,
  AiBillingLine,
  AiExchangeRate,
} from "@/lib/api/queries/ai-admin";
import type { AiSubscriptionMismatchList } from "@/lib/api/contracts/ai-usage";
import {
  billableTotals,
  formatBillable,
  formatConverted,
  unassignedLines,
} from "./billing";
import { formatCount, listOf, monthLabel, plural, shortDate } from "./format";
import {
  UPLOAD_VERDICTS,
  holdsBillBack,
  verdictCounts,
  type DeveloperUploads,
  type UploadVerdict,
} from "./upload-health";

/*
 * What holds a month's bill back before it is exported: uploads, exchange
 * rates, project mappings and undeclared plans. The server has already decided every fact; this
 * only says each in one line and whether it needs attention.
 */

/** Whether a check lets the month be exported. */
export type CheckStatus = "passed" | "attention" | "pending";

/** A check's state and its one-line summary. */
export type CheckResult = Readonly<{ status: CheckStatus; summary: string }>;

/** Verdicts whose missing days are not just a late start or a tool. */
const SEVERE: ReadonlySet<UploadVerdict> = new Set([
  "neverSynced",
  "holes",
  "nothingUploaded",
]);

/**
 * Whether every developer's machines uploaded all the month requires. A
 * month in progress whose latest day only waits for its next sync passes.
 */
export function uploadsCheck(
  completeness: AiBillingCompleteness,
  uploads: readonly DeveloperUploads[],
): CheckResult {
  if (uploads.length === 0) {
    return {
      status: "passed",
      summary: "Nobody syncs AI usage or has a subscription this month",
    };
  }
  if (completeness.requiredThrough === null) {
    return { status: "passed", summary: "No day of this month has passed yet" };
  }
  const counts = verdictCounts(uploads);
  const problems = UPLOAD_VERDICTS.filter(
    (verdict) => holdsBillBack(verdict) && counts[verdict] > 0,
  );
  if (problems.length === 0) {
    const since =
      counts.lateStart > 0
        ? `; ${formatCount(counts.lateStart)} started mid-month`
        : "";
    return {
      status: "passed",
      summary:
        counts.awaitingSync > 0
          ? `Everything is in, except days that wait for a sync${since}`
          : counts.lateStart > 0
            ? `Everything since each first upload is in${since}`
            : completeness.inProgress
              ? `Every day through ${shortDate(completeness.requiredThrough)} is uploaded`
              : "Every active machine uploaded every day",
    };
  }
  const gaps = problems.reduce((sum, verdict) => sum + counts[verdict], 0);
  const severe = problems
    .filter((verdict) => SEVERE.has(verdict))
    .reduce((sum, verdict) => sum + counts[verdict], 0);
  const of = `${formatCount(gaps)} of ${plural(uploads.length, "developer")}`;
  // The verb agrees with the count it follows: "1 of 3 developers misses".
  const verb = (count: number, one: string, many: string) =>
    count === 1 ? one : many;
  return {
    status: "attention",
    summary:
      severe === 0
        ? `${of} ${verb(gaps, "misses", "miss")} a tool on some days`
        : severe === gaps
          ? `${of} ${verb(gaps, "misses", "miss")} days mid-month or uploaded nothing`
          : `${of} ${verb(gaps, "has", "have")} gaps; ${formatCount(severe)} ${verb(severe, "misses", "miss")} days or uploaded nothing`,
  };
}

/**
 * Whether every paid plan the tools reported is declared: an undeclared one
 * bills its usage at API prices instead of a share of a fee. `list` must be
 * the evidence of the bill's month alone: the server groups evidence into runs
 * that may span days without any, so a wider search cannot say which months
 * had it.
 */
export function plansCheck(list: AiSubscriptionMismatchList): CheckResult {
  if (list.mismatches.length === 0) {
    return {
      status: "passed",
      summary: "Every paid plan the tools reported is declared",
    };
  }
  const developers = new Set(
    list.mismatches.map((mismatch) => mismatch.userId),
  );
  return {
    status: "attention",
    summary: `${plural(developers.size, "developer")} ${developers.size === 1 ? "uses" : "use"} a plan nobody declared, so it bills at API prices`,
  };
}

/** One rate as the summary names it, such as `USD 10.0169 (provisional)`. */
function rateLabel(rate: AiExchangeRate): string {
  const label = `${rate.currency} ${rate.rate}`;
  return rate.override
    ? `${label} (override)`
    : rate.provisional
      ? `${label} (provisional)`
      : label;
}

/**
 * Whether everything on the bill converts to the billing currency at a rate
 * that will not change. A missing rate and a provisional one need attention;
 * a rate being fetched is pending. Only the rates that convert an amount on
 * the bill count: a month keeps a stored rate after the last amount in its
 * currency is gone.
 */
export function ratesCheck({
  month,
  billingCurrency,
  lines,
  exchangeRates,
  missingRates,
  pendingRates,
}: {
  month: string;
  billingCurrency: string;
  lines: readonly AiBillingLine[];
  exchangeRates: readonly AiExchangeRate[];
  missingRates: readonly string[];
  pendingRates: readonly string[];
}): CheckResult {
  const missing = missingRates.filter((code) => !pendingRates.includes(code));
  if (missing.length > 0) {
    return {
      status: "attention",
      summary: `No ${listOf(missing, "disjunction")} rate for ${monthLabel(month)}: amounts in ${listOf(missing, "conjunction")} are not converted to ${billingCurrency}`,
    };
  }
  // A line of unknown or no cost has nothing to convert, whatever its
  // currency.
  const billed = new Set(
    lines
      .filter(
        (line) =>
          line.billableAmount !== null && Number(line.billableAmount) !== 0,
      )
      .map((line) => line.billableCurrency),
  );
  const rates = exchangeRates
    .filter((rate) => billed.has(rate.currency))
    .sort((a, b) => a.currency.localeCompare(b.currency));
  const summary = listOf(rates.map(rateLabel), "conjunction");
  // A provisional rate still converts the bill while its replacement is
  // fetched, so it stays open through the fetch.
  const provisional = rates.filter((rate) => rate.provisional);
  if (provisional.length > 0) {
    const refreshing = provisional
      .map((rate) => rate.currency)
      .filter((code) => pendingRates.includes(code));
    return {
      status: "attention",
      summary:
        refreshing.length > 0
          ? `${summary}; fetching ${listOf(refreshing, "conjunction")} again`
          : summary,
    };
  }
  if (pendingRates.length > 0) {
    return {
      status: "pending",
      summary: `Fetching the ${listOf(pendingRates, "conjunction")} exchange rate${pendingRates.length > 1 ? "s" : ""} for ${monthLabel(month)}`,
    };
  }
  if (rates.length === 0) {
    return {
      status: "passed",
      summary: `Everything is billed in ${billingCurrency}; no rate is needed`,
    };
  }
  return {
    status: "passed",
    summary: rates.some((rate) => rate.override)
      ? summary
      : `${summary}, ${rates.length > 1 ? "monthly averages" : "the monthly average"}`,
  };
}

/** What a set of lines bills, as a check names it: in the billing currency,
 * or the original amounts without a rate. */
function worthOf(
  lines: readonly AiBillingLine[],
  billingCurrency: string,
): string {
  const totals = billableTotals(lines);
  if (totals.amounts.size === 0) {
    return `${plural(lines.length, "line")} of unknown cost`;
  }
  return totals.converted === null
    ? formatBillable(totals)
    : formatConverted(totals, billingCurrency);
}

/**
 * Whether every repository the bill charges for is mapped to a project.
 * Usage of repositories nobody has mapped needs attention: mapping them on
 * Project mappings assigns it. So does a metered payer's code review of
 * GitHub repositories nobody has mapped, which mapping them under Project
 * mappings > GitHub repositories assigns, and which holds back month
 * close. Usage outside any repository has no key a mapping could name, so it
 * does not hold the bill back, though the summary still says no customer is
 * invoiced for it.
 */
export function mappingsCheck(
  lines: readonly AiBillingLine[],
  billingCurrency: string,
): CheckResult {
  const unmapped = unassignedLines(lines, "unmapped");
  const codeReview = unassignedLines(lines, "unmappedRepository");
  const unattributed = unassignedLines(lines, "unattributed");
  if (
    unmapped.length === 0 &&
    codeReview.length === 0 &&
    unattributed.length === 0
  ) {
    return {
      status: "passed",
      summary:
        lines.length === 0
          ? "Nothing is billed, so nothing needs a project"
          : "Every billed line has a project",
    };
  }
  const outside =
    unattributed.length > 0
      ? `${worthOf(unattributed, billingCurrency)} is outside any repository, which no mapping can assign`
      : null;
  if (unmapped.length === 0 && codeReview.length === 0) {
    // Code review is named only on a bill that has it: from the metered
    // cutover on, with a payer that bills repositories.
    const reviewed = lines.some((line) => line.metered?.kind === "repository");
    return {
      status: "passed",
      summary: reviewed
        ? `Every repository is mapped, GitHub's code review repositories too; ${outside}`
        : `Every repository is mapped; ${outside}`,
    };
  }
  const parts = [
    unmapped.length > 0
      ? `${worthOf(unmapped, billingCurrency)} comes from repositories nobody has mapped, so no customer can be invoiced for it yet`
      : null,
    codeReview.length > 0
      ? `${worthOf(codeReview, billingCurrency)} of code review comes from GitHub repositories nobody has mapped (Project mappings > GitHub repositories)`
      : null,
    outside,
  ];
  return {
    status: "attention",
    summary: parts.filter((part) => part !== null).join("; "),
  };
}
