import { z } from "zod";
import { AI_PROVIDERS } from "./ai-usage";

/*
 * Admin-only metered usage: what a metered payer (GitHub Copilot) billed,
 * imported per account, UTC day and model, and its reconciliation with Toki's
 * tracked usage. Days are UTC calendar days as `YYYY-MM-DD`; months are UTC
 * months as `YYYY-MM`; instants are RFC 3339.
 *
 * - Credits are strings with exactly two decimals, hundredths of a credit (a
 *   credit is $0.01). Only a residual and the `unknownAccount*` /
 *   `incompleteImport` totals can be negative.
 * - USD amounts are strings with two decimals.
 * - GitHub's raw quantities are exact decimal strings, never rounded.
 */

const count = z.number().int().nonnegative();
const id = z.number().int().positive();
const utcDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const monthSchema = z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/);
const instantSchema = z.string().datetime({ offset: true });
/** Hundredths of a credit, never negative: `12.34`. */
const creditsSchema = z.string().regex(/^\d+\.\d{2}$/);
/** Hundredths of a credit that may be negative: `-0.05`. */
const signedCreditsSchema = z.string().regex(/^-?\d+\.\d{2}$/);
/** Dollars with cents: `12.34`. */
const usdSchema = z.string().regex(/^\d+\.\d{2}$/);
/** An exact, non-negative decimal as GitHub gave it: `12.345678912`. */
const quantitySchema = z.string().regex(/^\d+(\.\d+)?$/);

export const METERED_SOURCES = [
  "accountCredits",
  "orgCredits",
  "orgUsage",
  "summary",
] as const;
export type AiMeteredSourceKind = (typeof METERED_SOURCES)[number];

export const METERED_FLAGS = [
  "trackedNotBilled",
  "lowRatio",
  "stalePricing",
  "unknownModel",
  "unlinked",
  "external",
  "payerTotalMismatch",
  "importFailed",
] as const;
export type AiMeteredFlag = (typeof METERED_FLAGS)[number];

export const METERED_GROUP_KINDS = [
  "trackable",
  "untrackable",
  "repositoryBilled",
  "unrecognized",
  "trackedOnly",
] as const;
export type AiMeteredGroupKind = (typeof METERED_GROUP_KINDS)[number];

export const METERED_CHARGE_KINDS = [
  "repositoryCredits",
  "accountCreditsAggregate",
  "untrackableCredits",
  "compute",
  "seats",
  "unrecognized",
] as const;
export type AiMeteredChargeKind = (typeof METERED_CHARGE_KINDS)[number];

export const RESIDUAL_STATUSES = [
  "settled",
  "provisional",
  "incomplete",
] as const;
export type AiResidualStatus = (typeof RESIDUAL_STATUSES)[number];

const flagSchema = z.enum(METERED_FLAGS);

const runFields = {
  runId: id,
  kind: z.enum(["hourly", "nightly", "admin"]),
  /** The months imported, oldest first. */
  months: z.array(monthSchema),
  startedAt: instantSchema,
  finishedAt: instantSchema.nullable(),
  status: z.enum(["running", "succeeded", "partial", "failed"]),
  requests: count,
  slicesFetched: count,
  slicesChanged: count,
  accountErrors: count,
  error: z.string().nullable(),
};

const runSchema = z.object(runFields).strict();

/** Why the import is stale, for the page to phrase. */
const staleReasonSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("lastRunFailed"), error: z.string() }).strict(),
  z.object({ kind: z.literal("neverSucceeded") }).strict(),
  z.object({ kind: z.literal("lastSuccessOld"), at: instantSchema }).strict(),
  z
    .object({
      kind: z.literal("sourceNotImported"),
      source: z.enum(METERED_SOURCES),
    })
    .strict(),
  z
    .object({
      kind: z.literal("sourceBehind"),
      source: z.enum(METERED_SOURCES),
      /** The last UTC day imported. */
      through: utcDateSchema,
    })
    .strict(),
  z.object({ kind: z.literal("summaryNeverFetched") }).strict(),
  z.object({ kind: z.literal("summaryOld"), at: instantSchema }).strict(),
  z
    .object({
      kind: z.literal("accountFailed"),
      login: z.string().min(1),
      month: monthSchema,
      error: z.string(),
    })
    .strict(),
]);

const importSchema = z
  .object({
    status: z.enum(["ready", "disabled", "misconfigured"]),
    /** Why the source is misconfigured; null otherwise. */
    message: z.string().nullable(),
    /** The first UTC day the import covers. */
    backfillFrom: utcDateSchema,
    /** Whether scheduled imports run in this build. */
    unattended: z.boolean(),
    freshness: z.enum(["fresh", "stale", "never", "off"]),
    staleReason: staleReasonSchema.nullable(),
    running: runSchema
      .refine((run) => run.status === "running" && run.finishedAt === null, {
        message: "a running run is running and unfinished",
      })
      .nullable(),
    lastRun: runSchema
      .refine((run) => run.status !== "running" && run.finishedAt !== null, {
        message: "the last run is finished",
      })
      .nullable(),
    /** The latest run's account errors, one per account and UTC month;
     * errors of accounts without a seat in that month are informational. */
    accountErrors: z.array(
      z
        .object({
          login: z.string().min(1),
          month: monthSchema,
          error: z.string(),
          seated: z.boolean(),
        })
        .strict(),
    ),
    sources: z.array(
      z
        .object({
          source: z.enum(METERED_SOURCES),
          importedThrough: utcDateSchema.nullable(),
          lastSuccessAt: instantSchema.nullable(),
          lastError: z.string().nullable(),
          settleLagDays: count.nullable(),
        })
        .strict(),
    ),
  })
  .strict()
  .refine(
    (overview) =>
      (overview.freshness === "stale") === (overview.staleReason !== null),
    { message: "only a stale import has a stale reason" },
  );

const poolSchema = z
  .object({
    fetchedAt: instantSchema,
    /** The first UTC day with net credits: the pool ran out. */
    exhaustedOn: utcDateSchema.nullable(),
    credits: z
      .object({
        grossCredits: creditsSchema,
        discountCredits: creditsSchema,
        netCredits: creditsSchema,
        grossUsd: usdSchema,
        discountUsd: usdSchema,
        netUsd: usdSchema,
      })
      .strict(),
    seats: z
      .object({
        userMonths: quantitySchema,
        grossUsd: usdSchema,
        netUsd: usdSchema,
      })
      .strict()
      .nullable(),
    /** The payer's bill for the month so far: every row's net amount. */
    payerNetUsd: usdSchema,
    unrecognized: z.array(
      z
        .object({ product: z.string(), sku: z.string(), unit: z.string() })
        .strict(),
    ),
  })
  .strict();

const totalsSchema = z
  .object({
    gross: creditsSchema,
    repositoryBilled: creditsSchema,
    tracked: creditsSchema,
    trackedOldBasis: creditsSchema,
    trackedBeforeFloor: creditsSchema,
    attributed: creditsSchema,
    untracked: creditsSchema,
    unlinked: creditsSchema,
    external: creditsSchema,
    /** Org total minus the per-account sum on settled and provisional days. */
    unknownAccount: signedCreditsSchema,
    unknownAccountSettled: signedCreditsSchema,
    /** The same on days the import has not covered: an import gap. */
    incompleteImport: signedCreditsSchema,
  })
  .strict();

const targetSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("user"), userId: id }).strict(),
  z.object({ kind: z.literal("external") }).strict(),
]);

const ownerSchema = z
  .object({ target: targetSchema, from: utcDateSchema, through: utcDateSchema })
  .strict()
  .refine((owner) => owner.from <= owner.through, {
    message: "an owner's first day is not after its last",
  });

/** The latest comparison of stored days with the payer's month total: a
 * failed check has an error and nothing to compare, any other check both. */
const checkSchema = z
  .object({
    checkedAt: instantSchema,
    /** The payer's month total of gross quantities, exactly. */
    payerGrossQuantity: quantitySchema.nullable(),
    /** The stored days' total, exactly. */
    storedGrossQuantity: quantitySchema,
    matches: z.boolean().nullable(),
    error: z.string().min(1).nullable(),
  })
  .strict()
  .refine(
    (check) =>
      check.error === null
        ? check.payerGrossQuantity !== null && check.matches !== null
        : check.payerGrossQuantity === null && check.matches === null,
    { message: "a check either failed with an error or compared totals" },
  );

const groupSchema = z
  .object({
    /** Null for a group with only Toki usage. */
    sku: z.string().nullable(),
    model: z.string(),
    /** GitHub's raw labels, such as `Auto: GPT-6.1 Sol`. */
    labels: z.array(z.string()),
    kind: z.enum(METERED_GROUP_KINDS),
    /** The model's catalog status; null when the catalog doesn't know it. */
    status: z.enum(["confirmed", "provisional", "free"]).nullable(),
    gross: creditsSchema,
    /** The part of `gross` GitHub billed as Auto. */
    autoGross: creditsSchema,
    tracked: creditsSchema,
    attributed: creditsSchema,
    untracked: creditsSchema,
    /** G / T on linked days; null without tracked usage. */
    ratio: z.number().nonnegative().nullable(),
    flags: z.array(flagSchema),
  })
  .strict();

const accountSchema = z
  .object({
    accountId: id,
    login: z.string().min(1),
    /** Whose the account's usage was, by day range within the month. */
    owners: z.array(ownerSchema),
    /** Days with credits and no disposition. */
    unlinkedDays: count,
    /** Days whose usage wasn't imported for the account: nothing compared. */
    daysNotImported: count,
    gross: creditsSchema,
    repositoryBilled: creditsSchema,
    untrackable: creditsSchema,
    tracked: creditsSchema,
    trackedOldBasis: creditsSchema,
    trackedBeforeFloor: creditsSchema,
    unpricedRecords: count,
    attributed: creditsSchema,
    untracked: creditsSchema,
    unlinked: creditsSchema,
    external: creditsSchema,
    /** Attributed over trackable gross on linked days; null without it. */
    trackedShare: z.number().nonnegative().nullable(),
    flags: z.array(flagSchema),
    payerCheck: checkSchema.nullable(),
    groups: z.array(groupSchema),
  })
  .strict();

const unlinkedTrackedSchema = z
  .object({ userId: id, tracked: creditsSchema, days: count })
  .strict();

const repositorySchema = z
  .object({
    /** The bare repository name; empty when the row names none. */
    repository: z.string(),
    product: z.string(),
    sku: z.string(),
    unit: z.string(),
    kind: z.enum(METERED_CHARGE_KINDS),
    quantity: quantitySchema,
    /** The quantity in credits, for credit rows only. */
    credits: creditsSchema.nullable(),
    grossUsd: usdSchema,
    discountUsd: usdSchema,
    netUsd: usdSchema,
    days: count,
  })
  .strict();

const residualDaySchema = z
  .object({
    day: utcDateSchema,
    orgGross: creditsSchema,
    accountsGross: creditsSchema,
    /** Organization minus accounts. */
    residual: signedCreditsSchema,
    status: z.enum(RESIDUAL_STATUSES),
    accountsExceedOrg: z.boolean(),
    groups: z.array(
      z
        .object({
          sku: z.string(),
          model: z.string(),
          residual: signedCreditsSchema,
        })
        .strict(),
    ),
  })
  .strict();

/** Code review on one reconciled UTC day, per account against per
 * repository: the same credits, which the bill takes once, per repository. */
const codeReviewDaySchema = z
  .object({
    day: utcDateSchema,
    byAccounts: creditsSchema,
    byRepositories: creditsSchema,
    /** Repositories minus accounts, `0.00` when they agree. Negative is code
     * review GitHub billed without a repository (inside the IDE aggregate),
     * which bills on no line, or in an unrecognized row naming a
     * repository. Positive is code review by accounts Toki does not import
     * (unknown accounts), which bills once, through its repository line. */
    difference: signedCreditsSchema,
    status: z.enum(RESIDUAL_STATUSES),
  })
  .strict();

const meteredMonthSchema = z
  .object({
    /** The metered payer, such as `copilot`. */
    provider: z.enum(AI_PROVIDERS),
    /** Where its accounts live, such as the GitHub organization. */
    namespace: z.string(),
    month: monthSchema,
    today: utcDateSchema,
    /** Months with imported data, oldest first. */
    months: z.array(monthSchema),
    import: importSchema,
    pool: poolSchema.nullable(),
    totals: totalsSchema,
    accounts: z.array(accountSchema),
    unlinkedTracked: z.array(unlinkedTrackedSchema),
    repositories: z.array(repositorySchema),
    /** Code review per account against per repository, over the month's
     * reconciled days; null without code review on either side. */
    codeReview: z
      .object({
        byAccounts: creditsSchema,
        byRepositories: creditsSchema,
        /** Every reconciled UTC day with code review on either side. */
        days: z.array(codeReviewDaySchema),
      })
      .strict()
      .nullable(),
    residualDays: z.array(residualDaySchema),
    /** The organization's stored month against the payer's month total,
     * which the residual rests on: no day settles until it matches, and a
     * failed check makes every day an import gap. Null in one developer's
     * view and before the first check. */
    orgCheck: checkSchema.nullable(),
  })
  .strict();

const importStartedSchema = z
  .object({
    runId: id,
    startedAt: instantSchema,
    /** The months the run imports: the requested one, plus months with gaps. */
    months: z.array(monthSchema).min(1),
  })
  .strict();

export type AiMeteredMonth = Readonly<z.infer<typeof meteredMonthSchema>>;
export type AiMeteredImport = Readonly<z.infer<typeof importSchema>>;
export type AiMeteredStaleReason = Readonly<z.infer<typeof staleReasonSchema>>;
export type AiMeteredAccountError = AiMeteredImport["accountErrors"][number];
export type AiMeteredRun = Readonly<z.infer<typeof runSchema>>;
export type AiMeteredPool = Readonly<z.infer<typeof poolSchema>>;
export type AiMeteredTotals = Readonly<z.infer<typeof totalsSchema>>;
export type AiMeteredAccount = Readonly<z.infer<typeof accountSchema>>;
export type AiMeteredOwner = Readonly<z.infer<typeof ownerSchema>>;
export type AiMeteredCheck = Readonly<z.infer<typeof checkSchema>>;
export type AiMeteredGroup = Readonly<z.infer<typeof groupSchema>>;
export type AiMeteredUnlinkedTracked = Readonly<
  z.infer<typeof unlinkedTrackedSchema>
>;
export type AiMeteredRepository = Readonly<z.infer<typeof repositorySchema>>;
export type AiResidualDay = Readonly<z.infer<typeof residualDaySchema>>;
export type AiCodeReviewDay = Readonly<z.infer<typeof codeReviewDaySchema>>;
export type AiMeteredImportStarted = Readonly<
  z.infer<typeof importStartedSchema>
>;
/** How much of a month an import refetches: changed days, or every day. */
export type AiMeteredImportMode = "changes" | "full";

/** Parses a month of metered usage at the HTTP boundary. */
export function parseMeteredMonth(input: unknown): AiMeteredMonth {
  return meteredMonthSchema.parse(input);
}

/** Parses the answer to starting an import. */
export function parseMeteredImportStarted(
  input: unknown,
): AiMeteredImportStarted {
  return importStartedSchema.parse(input);
}
