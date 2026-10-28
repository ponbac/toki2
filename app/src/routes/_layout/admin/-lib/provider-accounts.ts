import { parseSubscriptionDate } from "@/lib/ai-subscriptions";
import type {
  AiAcceptedSuggestions,
  AiAccountDispositionTerms,
  AiAccountPeriod,
  AiAccountSuggestion,
  AiAccountTarget,
  AiBillingDeveloper,
  AiMeteredActivity,
  AiProviderAccount,
  AiProviderSeat,
  AiSuggestionEvidence,
  AiSuggestionSkipReason,
} from "@/lib/api/queries/ai-admin";
import { formatEstimate, plural, shortDate } from "./format";

/*
 * Pure helpers for the GitHub logins page: provider accounts, their seats,
 * dispositions and suggestions. Days are UTC calendar days as `YYYY-MM-DD`,
 * which compare correctly as strings.
 */

/** Editors and tools Copilot reports as a seat's last client. */
const CLIENTS: readonly (readonly [RegExp, string])[] = [
  [/^vscode\//i, "VS Code"],
  [/^VS\/SSMS\./, "SQL Server Management Studio"],
  [/^(VS\/VisualStudio\.|VisualStudio\/)/, "Visual Studio"],
  [/^JetBrains/i, "JetBrains"],
  [/^Neovim\//i, "Neovim"],
  [/^vim\//i, "Vim"],
  [/^Xcode\//i, "Xcode"],
  [/^Eclipse\//i, "Eclipse"],
  [/^copilot-cli\b/i, "Copilot CLI"],
];

/**
 * A seat's last client, such as `vscode/1.143.0/copilot-chat/0.71.0`, as the
 * editor's name: `VS Code`. An unknown client reads as its first named part;
 * null stays null.
 */
export function clientLabel(client: string | null): string | null {
  const raw = client?.trim();
  if (!raw) {
    return null;
  }
  const known = CLIENTS.find(([pattern]) => pattern.test(raw));
  if (known) {
    return known[1];
  }
  const parts = raw.split("/").filter((part) => part !== "");
  return (
    parts.find((part) => part.toLowerCase() !== "unknown") ?? parts[0] ?? raw
  );
}

/** What the seat column shows about an account's seats. */
export type SeatSummary = Readonly<{
  /** `Since 2025-09-16`, `Ended 2026-10-31`, or `No seat`. */
  label: string;
  /** The latest seat, or null without any. */
  latest: AiProviderSeat | null;
  /** The day a pending cancellation ends the latest seat. */
  cancelsOn: string | null;
  ended: boolean;
  /** Seats before the latest. */
  earlier: number;
}>;

/** Summarizes an account's seats, oldest first, by its latest. */
export function seatSummary(seats: readonly AiProviderSeat[]): SeatSummary {
  const latest = seats.at(-1) ?? null;
  const earlier = Math.max(seats.length - 1, 0);
  if (latest === null) {
    return { label: "No seat", latest, cancelsOn: null, ended: false, earlier };
  }
  if (!latest.listed) {
    return {
      label: latest.lastDay ? `Ended ${latest.lastDay}` : "Ended",
      latest,
      cancelsOn: null,
      ended: true,
      earlier,
    };
  }
  return {
    label: `Since ${latest.firstDay}`,
    latest,
    cancelsOn: latest.pendingCancellationOn,
    ended: false,
    earlier,
  };
}

/** A seat's days: `2025-09-16 – ongoing`, or its last day once ended. */
export function seatPeriod(seat: AiProviderSeat): string {
  if (!seat.listed) {
    return `${seat.firstDay} – ${seat.lastDay ?? "ended"}`;
  }
  return seat.pendingCancellationOn
    ? `${seat.firstDay} – cancels ${seat.pendingCancellationOn}`
    : `${seat.firstDay} – ongoing`;
}

const EVIDENCE_LABELS: Record<AiSuggestionEvidence, string> = {
  email: "same email",
  name: "same name",
  nameFirstLast: "same first and last name",
  nameInitial: "same first name and initial",
  nameFirstOnly: "same first name",
  loginFullName: "login is the full name",
  loginInitialSurname: "login is initial and surname",
  loginFirstSurnamePrefix: "login is first name and surname start",
  loginFirstOnly: "login is the first name",
};

/** Why a suggestion was made, in words: `same name, login is the full name`. */
export function evidenceLabel(
  evidence: readonly AiSuggestionEvidence[],
): string {
  return evidence.map((item) => EVIDENCE_LABELS[item]).join(", ");
}

/** A disposition's days: `2023-04-17 – ongoing` or `… – 2026-10-31`. */
export function formatPeriod(period: AiAccountPeriod): string {
  return `${period.validFrom} – ${period.validTo ?? "ongoing"}`;
}

/** The day after `day`, both `YYYY-MM-DD`. */
export function nextDay(day: string): string {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(Date.UTC(year, month - 1, date + 1))
    .toISOString()
    .slice(0, 10);
}

/** The days from `from` through `through`, both included. */
export function daysIn(from: string, through: string): number {
  const toUtc = (day: string) => {
    const [year, month, date] = day.split("-").map(Number);
    return Date.UTC(year, month - 1, date);
  };
  return Math.round((toUtc(through) - toUtc(from)) / 86_400_000) + 1;
}

type DayRange = Readonly<{ from: string; through: string }>;

/** Inclusive day ranges in words: `Oct 16 – Oct 24, 9 days`. */
export function describeRanges(ranges: readonly DayRange[]): string {
  if (ranges.length === 0) {
    return "no days";
  }
  const days = ranges.reduce(
    (sum, range) => sum + daysIn(range.from, range.through),
    0,
  );
  const spans = ranges
    .map((range) =>
      range.from === range.through
        ? shortDate(range.from)
        : `${shortDate(range.from)} – ${shortDate(range.through)}`,
    )
    .join(", ");
  return `${spans} (${plural(days, "day")})`;
}

/**
 * What an account's credits in the review window still lack, in words: days
 * with credits that no disposition covers, and days with credits outside
 * every seat. Empty when nothing is missing.
 */
export function creditGaps(
  account: Pick<
    AiProviderAccount,
    "uncoveredCreditDays" | "creditDaysWithoutSeat"
  >,
): string[] {
  const gaps: string[] = [];
  if (account.uncoveredCreditDays.length > 0) {
    gaps.push(
      `Credits without a disposition: ${describeRanges(account.uncoveredCreditDays)}`,
    );
  }
  if (account.creditDaysWithoutSeat > 0) {
    gaps.push(
      `${plural(account.creditDaysWithoutSeat, "day")} with credits outside every seat`,
    );
  }
  return gaps;
}

/** An account to link to its verified top candidate. */
export type SuggestionPick = Readonly<{ accountId: number; userId: number }>;

/**
 * What "Accept all verified" links: the verified top candidate of each account
 * that has no disposition yet. Probable matches each need their own confirm,
 * and an account with any disposition is decided by hand.
 */
export function acceptAllPicks(
  accounts: readonly AiProviderAccount[],
): SuggestionPick[] {
  return accounts.flatMap((account) => {
    const top = account.suggestions[0];
    return account.dispositions.length === 0 && top?.confidence === "verified"
      ? [{ accountId: account.accountId, userId: top.userId }]
      : [];
  });
}

/** The target picker's value for "not a Toki user". */
export const EXTERNAL_TARGET = "external";

/** A target as the picker's value: `external` or `user:291`. */
export function targetValue(target: AiAccountTarget): string {
  return target.kind === "external"
    ? EXTERNAL_TARGET
    : `user:${String(target.userId)}`;
}

/** The target a picker value stands for; null for none or anything else. */
export function parseTargetValue(value: string): AiAccountTarget | null {
  if (value === EXTERNAL_TARGET) {
    return { kind: "external" };
  }
  const match = /^user:([1-9]\d*)$/.exec(value);
  return match ? { kind: "user", userId: Number(match[1]) } : null;
}

export const EXTERNAL_LABEL = "Not a Toki user";

/** Who a target is: the user's name, or "Not a Toki user". */
export function targetLabel(
  target: AiAccountTarget,
  names: ReadonlyMap<number, string>,
): string {
  return target.kind === "external"
    ? EXTERNAL_LABEL
    : userName(target.userId, names);
}

/** Whose an account's usage is, as a possessive: `Lisa Nord's`, or
 * `not a Toki user's`. */
export function ownerPhrase(
  target: AiAccountTarget,
  names: ReadonlyMap<number, string>,
): string {
  return target.kind === "external"
    ? "not a Toki user's"
    : `${userName(target.userId, names)}'s`;
}

/** A disposition's days in a sentence: `from 2026-04-20, ongoing`. */
export function periodPhrase(period: AiAccountPeriod): string {
  return period.validTo === null
    ? `from ${period.validFrom}, ongoing`
    : period.validFrom === period.validTo
      ? `on ${period.validFrom}`
      : `from ${period.validFrom} through ${period.validTo}`;
}

export function userName(
  userId: number,
  names: ReadonlyMap<number, string>,
): string {
  return names.get(userId) ?? `User ${String(userId)}`;
}

export type TargetOption = Readonly<{
  value: string;
  label: string;
  keywords?: string;
}>;

/**
 * The target picker's items, in order: the account's suggested candidates,
 * labelled with their confidence, then "Not a Toki user", then every other
 * user by name.
 */
export function targetOptions(
  suggestions: readonly AiAccountSuggestion[],
  developers: readonly AiBillingDeveloper[],
): TargetOption[] {
  const byId = new Map(developers.map((d) => [d.userId, d]));
  const suggested = suggestions.flatMap((suggestion) => {
    const developer = byId.get(suggestion.userId);
    return developer
      ? [
          {
            value: targetValue({ kind: "user", userId: developer.userId }),
            label: `${developer.fullName} · ${suggestion.confidence} match`,
            keywords: developer.email,
          },
        ]
      : [];
  });
  const suggestedIds = new Set(suggestions.map((s) => s.userId));
  const others = developers
    .filter((developer) => !suggestedIds.has(developer.userId))
    .map((developer) => ({
      value: targetValue({ kind: "user", userId: developer.userId }),
      label: developer.fullName,
      keywords: developer.email,
    }));
  return [
    ...suggested,
    { value: EXTERNAL_TARGET, label: EXTERNAL_LABEL },
    ...others,
  ];
}

/** A disposition as typed in its editor. */
export type DispositionDraft = Readonly<{
  /** A picker value; empty until chosen. */
  target: string;
  validFrom: string;
  /** Empty while ongoing. */
  validTo: string;
}>;

export type ParsedDisposition =
  | Readonly<{ ok: true; terms: AiAccountDispositionTerms }>
  | Readonly<{ ok: false; problem: string }>;

export function parseDispositionDraft(
  draft: DispositionDraft,
): ParsedDisposition {
  const target = parseTargetValue(draft.target);
  if (!target) {
    return {
      ok: false,
      problem: "Choose whose usage it is: a Toki user, or not a Toki user.",
    };
  }
  if (!parseSubscriptionDate(draft.validFrom)) {
    return { ok: false, problem: "Enter the first day as YYYY-MM-DD." };
  }
  if (draft.validTo && !parseSubscriptionDate(draft.validTo)) {
    return {
      ok: false,
      problem: "Enter the last day as YYYY-MM-DD, or leave it empty.",
    };
  }
  if (draft.validTo && draft.validTo < draft.validFrom) {
    return { ok: false, problem: "The last day must not be before the first." };
  }
  return {
    ok: true,
    terms: {
      target,
      validFrom: draft.validFrom,
      validTo: draft.validTo || null,
    },
  };
}

/**
 * The last day the End dialog offers: today, but never before the
 * disposition's first day, which the server would refuse.
 */
export function defaultEndDay(today: string, validFrom: string): string {
  return today < validFrom ? validFrom : today;
}

/** Why an End dialog's last day can't be saved, or null when it can. */
export function endDayProblem(day: string, validFrom: string): string | null {
  if (!parseSubscriptionDate(day)) {
    return "Enter the last day as YYYY-MM-DD.";
  }
  return day < validFrom
    ? `The last day can't be before its first, ${validFrom}.`
    : null;
}

/** Days the last successful read may lag today before the page flags it. */
const STALE_AFTER_DAYS = 2;

/**
 * Whether seats were last read successfully over two days before `today`,
 * comparing UTC days. Seats never read are not stale but missing, which the
 * page says instead.
 */
export function isStale(lastSuccessAt: string | null, today: string): boolean {
  if (lastSuccessAt === null) {
    return false;
  }
  const readOn = new Date(lastSuccessAt).toISOString().slice(0, 10);
  return daysIn(readOn, today) - 1 > STALE_AFTER_DAYS;
}

/** A user's metered usage in words: `Copilot-paid: 6 days, $12.30`. */
export function activityLabel(activity: AiMeteredActivity): string {
  return `Copilot-paid: ${plural(activity.days, "day")}, ${formatEstimate(activity.usage)}`;
}

const SKIP_REASONS: Record<AiSuggestionSkipReason, string> = {
  notVerified: "the match is only probable",
  changed: "the suggestion changed",
  alreadyDisposed: "the account already has a disposition",
  accountOverlap: "the days overlap another of its dispositions",
  userOverlap: "the user is linked to another account on those days",
};

/** What accepting suggestions did: `3 linked, 1 skipped (alice: …)`. */
export function acceptedSummary(
  accepted: AiAcceptedSuggestions,
  logins: ReadonlyMap<number, string>,
): string {
  const linked = `${String(accepted.created.length)} linked`;
  if (accepted.skipped.length === 0) {
    return linked;
  }
  const reasons = accepted.skipped
    .map(
      (skip) =>
        `${logins.get(skip.accountId) ?? `account ${String(skip.accountId)}`}: ${SKIP_REASONS[skip.reason]}`,
    )
    .join("; ");
  return `${linked}, ${String(accepted.skipped.length)} skipped (${reasons})`;
}
