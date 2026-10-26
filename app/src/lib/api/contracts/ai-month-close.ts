import { z } from "zod";
import { METERED_PROVISIONAL_REASONS } from "./ai-admin";
import { AI_PROVIDERS } from "./ai-usage";

/*
 * Month close contracts (admin only). An admin closes a month's whole AI bill
 * once it is over in UTC and the billing time zone, and the server's readiness
 * checks pass or are waived with a reason. A live import of the month is the
 * one problem no waiver covers, and the close itself is refused while any
 * usage import runs. A closed month is served from its snapshot; a later
 * change to it posts on the next open month as adjustment lines. A month can
 * be re-opened while no close holds adjustments to it.
 */

const providerSchema = z.enum(AI_PROVIDERS);
const id = z.number().int().positive();
const count = z.number().int().nonnegative();
/** A calendar date, `YYYY-MM-DD`. */
const dateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
/** A calendar month, `YYYY-MM`. */
const monthSchema = z.string().regex(/^\d{4}-\d{2}$/);
/** An instant, RFC 3339 with an offset. */
const instantSchema = z.string().datetime({ offset: true });
/** An exact amount with two decimals, such as `12.34`. */
const decimalSchema = z.string().regex(/^\d+\.\d{2}$/);
/** Credits as signed hundredths of a credit, such as `5.00` or `-5.00`. */
const creditsSchema = z
  .string()
  .regex(/^-?\d+\.\d{2}$/)
  .refine((value) => !/^-0+\.00$/.test(value), {
    message: "a zero is never signed",
  });
const currencySchema = z.string().regex(/^[A-Z]{3}$/);
const reasonsSchema = z.array(z.enum(METERED_PROVISIONAL_REASONS));

/** The readiness checks, in the order the server evaluates them. */
export const CLOSE_CHECKS = [
  "import",
  "dispositions",
  "unknownAccount",
  "repositories",
  "pricing",
  "exchangeRates",
  "uploads",
] as const;
export type AiCloseCheckId = (typeof CLOSE_CHECKS)[number];

/** Where a check stands: `waived` is failed, with an admin's reason. */
export const CLOSE_CHECK_STATUSES = [
  "passed",
  "failed",
  "waived",
  "notApplicable",
] as const;
export type AiCloseCheckStatus = (typeof CLOSE_CHECK_STATUSES)[number];

/** The most characters a waiver's reason may have, as the server counts
 * them (code points, after trimming). */
export const MAX_WAIVER_REASON_CHARS = 500;

/** How an account's or the organization's latest check fails: `failed` is
 * the import's own error, `interrupted` a run that stopped or died before
 * finishing the month (its `error` says why), whatever the account's seat. */
const outcomeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("missing") }).strict(),
  z.object({ kind: z.literal("failed"), error: z.string() }).strict(),
  z.object({ kind: z.literal("interrupted"), error: z.string() }).strict(),
  z.object({ kind: z.literal("differs") }).strict(),
  z
    .object({ kind: z.literal("beforeSettled"), checkedAt: instantSchema })
    .strict(),
]);

const gapSchema = z.object({ from: dateSchema, to: dateSchema }).strict();

/** A developer's machine a problem names. */
const machineShape = {
  userId: id,
  fullName: z.string(),
  machineId: z.string().uuid(),
  label: z.string(),
};

/** What a failed check found, one kind per problem. */
const problemSchema = z.discriminatedUnion("kind", [
  /** A live import run, started at `startedAt`, is importing the month. No
   * waiver covers it: its check stays failed until the run finishes. */
  z
    .object({ kind: z.literal("importRunning"), startedAt: instantSchema })
    .strict(),
  /** The payer may still change the month's last days until `settlesOn`. */
  z
    .object({
      kind: z.literal("notSettled"),
      lastDay: dateSchema,
      settlesOn: dateSchema,
      settleDays: count,
    })
    .strict(),
  /** Days before the import's backfill. */
  z
    .object({
      kind: z.literal("daysBeforeBackfill"),
      firstDay: dateSchema,
      lastDay: dateSchema,
    })
    .strict(),
  /** Days whose slices of a source are missing or incomplete. */
  z
    .object({
      kind: z.literal("daysNotImported"),
      source: z.enum(["accountCredits", "orgCredits", "orgUsage", "summary"]),
      days: count,
      firstDay: dateSchema,
      lastDay: dateSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("accountCheck"),
      accountId: id,
      login: z.string().min(1),
      outcome: outcomeSchema,
    })
    .strict(),
  z.object({ kind: z.literal("orgCheck"), outcome: outcomeSchema }).strict(),
  z
    .object({
      kind: z.literal("unlinkedCredits"),
      accountId: id,
      login: z.string().min(1),
      credits: creditsSchema,
    })
    .strict(),
  z
    .object({ kind: z.literal("residualNotSettled"), reasons: reasonsSchema })
    .strict(),
  /** A day's residual, signed: positive for accounts Toki never listed. */
  z
    .object({
      kind: z.literal("residual"),
      day: dateSchema,
      credits: creditsSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("unmappedRepository"),
      repository: z.string().min(1),
      credits: creditsSchema,
    })
    .strict(),
  z.object({ kind: z.literal("unrecognizedRepositoryRow") }).strict(),
  z
    .object({
      kind: z.literal("meteredPricing"),
      accountId: id,
      login: z.string().min(1),
      reasons: reasonsSchema,
    })
    .strict(),
  /** Buckets the bill reads on an old pricing basis, per machine and
   * provider, with their estimate in USD. */
  z
    .object({
      kind: z.literal("oldBasis"),
      ...machineShape,
      provider: providerSchema,
      buckets: count,
      firstDay: dateSchema,
      lastDay: dateSchema,
      estimateUsd: decimalSchema,
    })
    .strict(),
  z
    .object({ kind: z.literal("rateMissing"), currency: currencySchema })
    .strict(),
  z
    .object({ kind: z.literal("ratePending"), currency: currencySchema })
    .strict(),
  z
    .object({
      kind: z.literal("rateProvisional"),
      currency: currencySchema,
      observedThrough: dateSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("adjustedRateMissing"),
      month: monthSchema,
      currency: currencySchema,
    })
    .strict(),
  /** A corrected month's rate is still being fetched: the adjustments to
   * it may change once it arrives. */
  z
    .object({
      kind: z.literal("adjustedRatePending"),
      month: monthSchema,
      currency: currencySchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("adjustedRateProvisional"),
      month: monthSchema,
      currency: currencySchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("machineIncomplete"),
      ...machineShape,
      gaps: z.array(gapSchema),
      providers: z.array(
        z
          .object({ provider: providerSchema, gaps: z.array(gapSchema) })
          .strict(),
      ),
    })
    .strict(),
  /** UTC hours of a metered month outside its local days that a machine (or
   * one of its providers) did not upload. */
  z
    .object({
      kind: z.literal("utcHoursNotUploaded"),
      ...machineShape,
      provider: providerSchema.nullable(),
      from: instantSchema,
      until: instantSchema,
    })
    .strict(),
]);

const checkSchema = z
  .object({
    id: z.enum(CLOSE_CHECKS),
    status: z.enum(CLOSE_CHECK_STATUSES),
    problems: z.array(problemSchema),
    /** Exactly when the check is waived. */
    waiver: z
      .object({ reason: z.string().min(1) })
      .strict()
      .nullable(),
  })
  .strict()
  .refine((check) => (check.waiver !== null) === (check.status === "waived"), {
    message: "a waiver must be set exactly on a waived check",
  });

/** A month's readiness to close, as the server evaluated it. */
const readinessSchema = z
  .object({
    evaluatedAt: instantSchema,
    /** No check is failed without a waiver (a check holding a running
     * import stays failed whatever is waived). */
    ready: z.boolean(),
    /** In evaluation order. */
    checks: z.array(checkSchema),
  })
  .strict()
  .refine(
    (readiness) =>
      readiness.ready ===
      readiness.checks.every((check) => check.status !== "failed"),
    { message: "ready must be set exactly when no check is failed" },
  )
  .refine(
    (readiness) =>
      new Set(readiness.checks.map((check) => check.id)).size ===
      readiness.checks.length,
    { message: "each check appears once" },
  );

const adjustedMonthSchema = z
  .object({ month: monthSchema, snapshotId: id })
  .strict();

/** Where a month stands with close. */
const statusSchema = z
  .object({
    month: monthSchema,
    state: z.enum(["open", "closed"]),
    timeZone: z.string().min(1),
    /** An open month that is not over yet: when it is. */
    overAt: instantSchema.nullable(),
    /** An open month that is over: its readiness now. */
    readiness: readinessSchema.nullable(),
    /** An open month: the closed months whose changes post on it. */
    adjusts: z.array(adjustedMonthSchema),
    /** A closed month's close. */
    close: z
      .object({
        closeId: id,
        closedAt: instantSchema,
        closedBy: z.string().nullable(),
        /** The readiness as it was when the month closed, with waivers. */
        readiness: readinessSchema,
        /** Where later changes to the month post. */
        adjustmentsPostOn: monthSchema,
        /** Months whose closes hold adjustments to this one: it can be
         * re-opened once they are. */
        heldBy: z.array(monthSchema),
      })
      .strict()
      .nullable(),
    /** Every close of the month, re-opened ones included, newest first. */
    history: z.array(
      z
        .object({
          closeId: id,
          closedAt: instantSchema,
          closedBy: z.string().nullable(),
          reopenedAt: instantSchema.nullable(),
          reopenedBy: z.string().nullable(),
        })
        .strict(),
    ),
  })
  .strict()
  .refine(
    (status) =>
      status.state === "closed"
        ? status.close !== null &&
          status.overAt === null &&
          status.readiness === null &&
          status.adjusts.length === 0
        : status.close === null &&
          (status.overAt === null) !== (status.readiness === null),
    {
      message:
        "a closed month has its close; an open one is either in progress or has its readiness",
    },
  );

/** Why a close was refused: not ready (with the readiness the server just
 * evaluated), held by adjustments (re-open), or another conflict (not over,
 * already closed, busy, or a usage import running). */
const conflictSchema = z.union([
  z.object({ error: z.string(), readiness: readinessSchema }).strict(),
  z.object({ error: z.string(), heldBy: z.array(monthSchema) }).strict(),
  z.object({ error: z.string() }).strict(),
]);

export type AiCloseProblem = Readonly<z.infer<typeof problemSchema>>;
export type AiCloseProblemKind = AiCloseProblem["kind"];
export type AiCloseCheck = Readonly<z.infer<typeof checkSchema>>;
export type AiCloseReadiness = Readonly<z.infer<typeof readinessSchema>>;
export type AiMonthCloseStatus = Readonly<z.infer<typeof statusSchema>>;
export type AiCloseHistoryEntry = AiMonthCloseStatus["history"][number];
export type AiCloseConflict =
  | Readonly<{ kind: "notReady"; error: string; readiness: AiCloseReadiness }>
  | Readonly<{ kind: "heldBy"; error: string; heldBy: readonly string[] }>
  | Readonly<{ kind: "conflict"; error: string }>;

/** Parses where a month stands with close at the HTTP boundary. */
export function parseAiMonthCloseStatus(input: unknown): AiMonthCloseStatus {
  return statusSchema.parse(input);
}

/** Parses a refused close or re-open (`409`); null for any other body. */
export function parseAiCloseConflict(input: unknown): AiCloseConflict | null {
  const parsed = conflictSchema.safeParse(input);
  if (!parsed.success) {
    return null;
  }
  const body = parsed.data;
  if ("readiness" in body) {
    return { kind: "notReady", error: body.error, readiness: body.readiness };
  }
  if ("heldBy" in body) {
    return { kind: "heldBy", error: body.error, heldBy: body.heldBy };
  }
  return { kind: "conflict", error: body.error };
}

/** Whether a waiver of its check can let the month close, as the server
 * judges it: a live import of the month cannot be waived. */
export function problemWaivable(problem: AiCloseProblem): boolean {
  return problem.kind !== "importRunning";
}

/** A failed check a waiver can let through: the server drops a waiver of a
 * check holding a problem no waiver covers, which keeps it failed. */
export function checkWaivable(check: AiCloseCheck): boolean {
  return check.status === "failed" && check.problems.every(problemWaivable);
}

/** When the live import importing the month started, while one is: the
 * month cannot close, whatever is waived, until it finishes. */
export function importRunningSince(readiness: AiCloseReadiness): string | null {
  for (const check of readiness.checks) {
    for (const problem of check.problems) {
      if (problem.kind === "importRunning") {
        return problem.startedAt;
      }
    }
  }
  return null;
}

/** A waiver's reason as the server takes it: trimmed, 1 to 500 characters,
 * no control characters but line breaks. Null when it is acceptable. */
export function waiverReasonProblem(reason: string): string | null {
  const trimmed = reason.trim();
  const length = [...trimmed].length;
  if (length === 0) {
    return "Give a reason for waiving this check.";
  }
  if (length > MAX_WAIVER_REASON_CHARS) {
    return `Keep the reason to ${MAX_WAIVER_REASON_CHARS} characters.`;
  }
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/.test(trimmed)) {
    return "The reason must not contain control characters.";
  }
  return null;
}
