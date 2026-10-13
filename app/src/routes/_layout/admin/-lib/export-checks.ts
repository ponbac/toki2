import type {
  AiBillingCompleteness,
  AiBillingLine,
  AiExchangeRate,
} from "@/lib/api/queries/ai-admin";
import {
  billableTotals,
  formatBillable,
  formatConverted,
  unassignedLines,
} from "./billing";
import { coverageTotals, monthDays, needsAttention } from "./coverage";
import { listOf, monthLabel, plural } from "./format";

/*
 * What holds a month's bill back before it is exported: uploads, exchange
 * rates and project mappings. The server has already decided every fact; this
 * only says each in one line and whether it needs attention.
 */

/** Whether a check lets the month be exported. */
export type CheckStatus = "passed" | "attention" | "pending";

/** A check's state and its one-line summary. */
export type CheckResult = Readonly<{ status: CheckStatus; summary: string }>;

/** Whether every developer's machines uploaded all the month requires. */
export function uploadsCheck(completeness: AiBillingCompleteness): CheckResult {
  const developers = completeness.developers;
  if (developers.length === 0) {
    return {
      status: "passed",
      summary:
        "Nobody has a machine that syncs AI usage or a subscription in this month",
    };
  }
  const attention = developers.filter(needsAttention);
  if (attention.length === 0) {
    return {
      status: "passed",
      summary:
        completeness.requiredThrough === null
          ? "No day of this month has passed yet"
          : completeness.inProgress
            ? "Every required day so far is uploaded; the month is still in progress"
            : "Every active machine has uploaded every required day",
    };
  }

  const totals = coverageTotals(completeness, monthDays(completeness.month));
  const notSynced = attention.filter(
    (developer) => developer.readiness === "noSync",
  ).length;
  const parts = [
    `${developers.length - attention.length} of ${plural(developers.length, "developer")} complete`,
  ];
  if (totals.missing > 0) {
    parts.push(`${plural(totals.missing, "machine-day")} missing`);
  } else if (totals.providerOnly > 0) {
    parts.push(
      `a tool is missing on ${plural(totals.providerOnly, "machine-day")}`,
    );
  } else if (notSynced > 0) {
    parts.push(`${plural(notSynced, "developer")} not synced`);
  }
  return { status: "attention", summary: parts.join(", ") };
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

/** Whether every billed line is charged to a project. */
export function mappingsCheck(
  lines: readonly AiBillingLine[],
  billingCurrency: string,
): CheckResult {
  const unassigned = unassignedLines(lines);
  if (unassigned.length === 0) {
    return {
      status: "passed",
      summary:
        lines.length === 0
          ? "Nothing is billed, so nothing needs a project"
          : "Every billed line has a project",
    };
  }
  const totals = billableTotals(unassigned);
  const count = `${plural(unassigned.length, "line")} unassigned`;
  if (totals.amounts.size === 0) {
    return { status: "attention", summary: count };
  }
  // Without a rate there is no converted sum: name the original amounts.
  const worth =
    totals.converted === null
      ? formatBillable(totals)
      : formatConverted(totals, billingCurrency);
  return { status: "attention", summary: `${count}, worth ${worth}` };
}
