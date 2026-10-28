import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiCloseCheck,
  AiCloseCheckId,
  AiCloseProblem,
  AiCloseReadiness,
} from "@/lib/api/queries/ai-admin";
import { PROVISIONAL_REASON_LABELS, formatLineCredits } from "./billing";
import { formatInstant, listOf, monthLabel, plural, shortDate } from "./format";

/*
 * Wording for month close: each readiness check's name, what it checks, and
 * each problem the server found, as a sentence. The server decides every
 * check; this only describes them.
 */

/** Each check: its name, and what passing it means. */
export const CLOSE_CHECK_LABELS = {
  import: {
    label: "GitHub import",
    passed:
      "GitHub's usage is imported, settled and checked against its month totals.",
  },
  dispositions: {
    label: "Account dispositions",
    passed: "No credits bill to a GitHub account without a disposition.",
  },
  unknownAccount: {
    label: "Unknown account",
    passed: "The accounts explain GitHub's organization total on every day.",
  },
  repositories: {
    label: "Repositories",
    passed: "Every repository with code review credits is mapped.",
  },
  pricing: {
    label: "Pricing",
    passed:
      "No billed estimate or Copilot line rests on an old pricing basis or a provisional model.",
  },
  exchangeRates: {
    label: "Exchange rates",
    passed: "Every rate the bill converts with is final.",
  },
  uploads: {
    label: "Uploads",
    passed: "Every machine active in the month uploaded all of it.",
  },
} as const satisfies Record<AiCloseCheckId, { label: string; passed: string }>;

/** What a check that does not apply to the month says. */
export const NOT_APPLICABLE_NOTE =
  "Does not apply: the month bills GitHub Copilot from estimates.";

const SOURCE_LABELS = {
  accountCredits: "account credits",
  orgCredits: "organization credits",
  orgUsage: "organization usage",
  summary: "the usage summary",
} as const;

/** Inclusive days as `Sep 3` or `Sep 3 – Sep 9`. */
function days(first: string, last: string): string {
  return first === last
    ? shortDate(first)
    : `${shortDate(first)} – ${shortDate(last)}`;
}

function checkOutcome(
  outcome: Extract<AiCloseProblem, { kind: "orgCheck" }>["outcome"],
): string {
  switch (outcome.kind) {
    case "missing":
      return "never compared with GitHub's month total";
    case "failed":
      return `the comparison with GitHub failed: ${outcome.error}`;
    case "interrupted":
      return `${outcome.error}; the next run imports it again`;
    case "differs":
      return "the stored days differ from GitHub's month total";
    case "beforeSettled":
      return `last compared ${formatInstant(outcome.checkedAt)}, before the month settled; import it again`;
  }
}

function reasons(
  list: Extract<AiCloseProblem, { kind: "meteredPricing" }>["reasons"],
): string {
  return list.map((reason) => PROVISIONAL_REASON_LABELS[reason]).join("; ");
}

/** A machine as a problem names it: `Ada Lovelace · laptop`. */
function machine(problem: { fullName: string; label: string }): string {
  return `${problem.fullName} · ${problem.label || "unnamed machine"}`;
}

/** One problem a check found, as a sentence. */
export function problemText(problem: AiCloseProblem): string {
  switch (problem.kind) {
    case "importRunning":
      return `An import of the month is running (started ${formatInstant(problem.startedAt)}): wait for it to finish.`;
    case "notSettled":
      return `GitHub may still change the month's last days: it settles on ${shortDate(problem.settlesOn)} (${plural(problem.settleDays, "day")} after ${shortDate(problem.lastDay)}).`;
    case "daysBeforeBackfill":
      return `${days(problem.firstDay, problem.lastDay)} are before the import's backfill.`;
    case "daysNotImported":
      return `${plural(problem.days, "day")} of ${SOURCE_LABELS[problem.source]} not imported completely (${days(problem.firstDay, problem.lastDay)}).`;
    case "accountCheck":
      return `${problem.login}: ${checkOutcome(problem.outcome)}.`;
    case "orgCheck":
      return `The organization: ${checkOutcome(problem.outcome)}.`;
    case "unlinkedCredits":
      return `${problem.login}: ${formatLineCredits(problem.credits)} on days the account has no disposition.`;
    case "residualNotSettled":
      return `The unknown account's residual may still change: ${reasons(problem.reasons)}.`;
    case "residual":
      return `${shortDate(problem.day)}: ${formatLineCredits(problem.credits)} the accounts do not explain.`;
    case "unmappedRepository":
      return `${problem.repository}: ${formatLineCredits(problem.credits)} of code review bill as Unassigned.`;
    case "unrecognizedRepositoryRow":
      return "An organization row names a repository under a SKU or unit Toki does not recognize; it may be code review on no line.";
    case "meteredPricing":
      return `${problem.login}: ${reasons(problem.reasons)}.`;
    case "oldBasis":
      return `${machine(problem)} · ${AI_PROVIDER_LABELS[problem.provider]}: ${plural(problem.buckets, "bucket")} on an old pricing basis (${days(problem.firstDay, problem.lastDay)}), estimated at ${problem.estimateUsd} USD.`;
    case "rateMissing":
      return `No ${problem.currency} exchange rate.`;
    case "ratePending":
      return `The ${problem.currency} exchange rate is still being fetched.`;
    case "rateProvisional":
      return `The ${problem.currency} exchange rate is provisional: an average of the days through ${shortDate(problem.observedThrough)}.`;
    case "adjustedRateMissing":
      return `No ${problem.currency} exchange rate for ${monthLabel(problem.month)}, whose adjustments this month carries.`;
    case "adjustedRatePending":
      return `${monthLabel(problem.month)}'s ${problem.currency} exchange rate is still being fetched, and this month's adjustments to it may change.`;
    case "adjustedRateProvisional":
      return `${monthLabel(problem.month)}'s ${problem.currency} exchange rate is provisional, and this month's adjustments to it carry it.`;
    case "machineIncomplete": {
      const gaps = problem.gaps.map((gap) => days(gap.from, gap.to));
      const providers = problem.providers.map(
        (provider) => AI_PROVIDER_LABELS[provider.provider],
      );
      return `${machine(problem)}: not every day uploaded${gaps.length > 0 ? ` (${listOf(gaps, "conjunction")})` : ""}${providers.length > 0 ? `; ${listOf(providers, "conjunction")} missing days` : ""}.`;
    }
    case "utcHoursNotUploaded":
      return `${machine(problem)}${problem.provider === null ? "" : ` · ${AI_PROVIDER_LABELS[problem.provider]}`}: the UTC hours ${formatInstant(problem.from)} – ${formatInstant(problem.until)} are not uploaded.`;
  }
}

/** Where a check's problems are resolved: a tab of the AI billing page, or
 * another admin page. */
export type CloseCheckTarget =
  "copilot" | "data" | "github-logins" | "github-repositories";

/** Where each of a failed check's problems is resolved, in order, without
 * repeats. */
export function checkTargets(check: AiCloseCheck): CloseCheckTarget[] {
  const targets = check.problems.map((problem): CloseCheckTarget => {
    switch (check.id) {
      case "import":
      case "unknownAccount":
        return "copilot";
      case "dispositions":
        return "github-logins";
      case "repositories":
        return problem.kind === "unrecognizedRepositoryRow"
          ? "copilot"
          : "github-repositories";
      case "pricing":
        return problem.kind === "oldBasis" ? "data" : "copilot";
      case "exchangeRates":
      case "uploads":
        return "data";
    }
  });
  return [...new Set(targets)];
}

/** The more recently evaluated of two readiness answers. */
export function latestReadiness(
  a: AiCloseReadiness | null,
  b: AiCloseReadiness | null,
): AiCloseReadiness | null {
  if (a === null || b === null) {
    return a ?? b;
  }
  return Date.parse(b.evaluatedAt) > Date.parse(a.evaluatedAt) ? b : a;
}
