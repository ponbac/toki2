import { z } from "zod";
import { AI_PROVIDERS } from "./ai-usage";

/*
 * Admin-only AI usage contracts: monthly billing, data completeness, and the
 * team-wide project-key mappings. Billing responses are never finer than one
 * local day per developer; they carry no hours or sessions.
 */

const providerSchema = z.enum(AI_PROVIDERS);
const count = z.number().int().nonnegative();
const id = z.number().int().positive();
/** A calendar date in the server's AI usage time zone, `YYYY-MM-DD`. */
const localDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
/** A calendar month, `YYYY-MM`. */
const monthSchema = z.string().regex(/^\d{4}-\d{2}$/);
/** An exact amount with two decimals, such as `550.00`. */
const decimalSchema = z.string().regex(/^\d+\.\d{2}$/);
const currencySchema = z.string().regex(/^[A-Z]{3}$/);

const projectSchema = z
  .object({ projectId: z.string().min(1), projectName: z.string() })
  .strict();

const developerSchema = z
  .object({ userId: id, fullName: z.string(), email: z.string() })
  .strict();

const usageSchema = z
  .object({
    apiEquivalentUsd: z.number().nonnegative(),
    unpricedRecords: count,
    records: count,
    tokens: z
      .object({
        input: count,
        cacheRead: count,
        cacheWrite: count,
        output: count,
        total: count,
      })
      .strict(),
  })
  .strict();

const billingLineSchema = z
  .object({
    userId: id,
    provider: providerSchema,
    project: projectSchema.nullable(),
    billingMode: z.enum(["api", "subscription"]),
    unallocatedOverhead: z.boolean(),
    subscriptionId: id.nullable(),
    /** What the line bills in `billableCurrency`: a fee share, or an API
     * estimate rounded to whole cents; null when its cost is unknown. */
    billableAmount: decimalSchema.nullable(),
    billableCurrency: currencySchema,
    usage: usageSchema,
  })
  .strict();

const subscriptionMonthSchema = z
  .object({
    subscriptionId: id,
    userId: id,
    provider: providerSchema,
    plan: z.string().min(1),
    monthlyCost: decimalSchema,
    currency: currencySchema,
    validFrom: localDateSchema,
    validTo: localDateSchema.nullable(),
    coveredFrom: localDateSchema,
    coveredTo: localDateSchema,
    coveredDays: z.number().int().positive(),
    daysInMonth: z.number().int().positive(),
    proratedFee: decimalSchema,
    allocation: z.enum(["apiCost", "tokens", "records", "unallocated"]),
    usage: usageSchema,
  })
  .strict();

const totalsSchema = z
  .object({
    fees: z.array(
      z
        .object({
          currency: currencySchema,
          billed: decimalSchema,
          overhead: decimalSchema,
        })
        .strict(),
    ),
    /** The sum of the API lines' rounded amounts, an exact decimal. */
    apiBilledUsd: decimalSchema,
    apiUnpricedRecords: count,
    usage: usageSchema,
  })
  .strict();

const periodShape = {
  month: monthSchema,
  firstDay: localDateSchema,
  lastDay: localDateSchema,
  timeZone: z.string().min(1),
};

const billingOverviewSchema = z
  .object({
    ...periodShape,
    developers: z.array(developerSchema),
    lines: z.array(billingLineSchema),
    subscriptions: z.array(subscriptionMonthSchema),
    totals: totalsSchema,
  })
  .strict();

const dayUsageSchema = z
  .object({
    day: localDateSchema,
    provider: providerSchema,
    model: z.string(),
    machineId: z.string().uuid(),
    projectKey: z.string().min(1),
    project: projectSchema.nullable(),
    subscriptionId: id.nullable(),
    usage: usageSchema,
  })
  .strict();

const developerMonthSchema = z
  .object({
    ...periodShape,
    developer: developerSchema,
    lines: z.array(billingLineSchema),
    subscriptions: z.array(subscriptionMonthSchema),
    totals: totalsSchema,
    machines: z.array(
      z.object({ machineId: z.string().uuid(), label: z.string() }).strict(),
    ),
    dailyUsage: z.array(dayUsageSchema),
  })
  .strict();

const gapSchema = z
  .object({ from: localDateSchema, to: localDateSchema })
  .strict();

const providerCompletenessSchema = z
  .object({
    provider: providerSchema,
    status: z.enum(["ok", "partial", "failed", "missing"]).nullable(),
    pricingStatus: z
      .enum(["fresh", "cached", "unavailable", "custom"])
      .nullable(),
    expected: z.boolean(),
    gaps: z.array(gapSchema),
  })
  .strict();

/** A machine's uploads for a month, by local day: never a time of day. */
const machineCompletenessSchema = z
  .object({
    machineId: z.string().uuid(),
    label: z.string(),
    clientVersion: z.string(),
    lastSyncedOn: localDateSchema,
    stale: z.boolean(),
    activeInMonth: z.boolean(),
    complete: z.boolean(),
    gaps: z.array(gapSchema),
    providers: z.array(providerCompletenessSchema),
  })
  .strict();

const completenessSchema = z
  .object({
    month: monthSchema,
    requiredThrough: localDateSchema.nullable(),
    inProgress: z.boolean(),
    staleAfterDays: z.number().int().positive(),
    developers: z.array(
      developerSchema
        .extend({
          readiness: z.enum(["ready", "incomplete", "noSync", "noActivity"]),
          hasUsage: z.boolean(),
          hasSubscription: z.boolean(),
          machines: z.array(machineCompletenessSchema),
        })
        .strict(),
    ),
  })
  .strict();

const unmappedKeySchema = z
  .object({
    projectKey: z.string().min(1),
    lastUsedOn: localDateSchema,
    mappable: z.boolean(),
  })
  .strict();

export type AiBillingDeveloper = Readonly<z.infer<typeof developerSchema>>;
export type AiBillingProject = Readonly<z.infer<typeof projectSchema>>;
/** Summed usage; `apiEquivalentUsd` covers priced records only. */
export type AiBillingUsage = Readonly<z.infer<typeof usageSchema>>;
/** One charge of a developer's usage of a provider to a project. */
export type AiBillingLine = Readonly<z.infer<typeof billingLineSchema>>;
/** A subscription's pro-rated fee for a month and how it was split. */
export type AiSubscriptionMonth = Readonly<
  z.infer<typeof subscriptionMonthSchema>
>;
export type AiBillingTotals = Readonly<z.infer<typeof totalsSchema>>;
export type AiBillingOverview = Readonly<z.infer<typeof billingOverviewSchema>>;
/** A developer's usage on one local day of one provider, model, machine and key. */
export type AiDayUsage = Readonly<z.infer<typeof dayUsageSchema>>;
export type AiDeveloperMonth = Readonly<z.infer<typeof developerMonthSchema>>;
export type AiBillingCompleteness = Readonly<
  z.infer<typeof completenessSchema>
>;
export type AiDeveloperCompleteness =
  AiBillingCompleteness["developers"][number];
export type AiMachineCompleteness = Readonly<
  z.infer<typeof machineCompletenessSchema>
>;
/** Inclusive local days that were not uploaded. */
export type AiUploadGap = Readonly<z.infer<typeof gapSchema>>;
export type AiUnmappedProjectKey = Readonly<z.infer<typeof unmappedKeySchema>>;

/** Parses a month's billing overview at the HTTP boundary. */
export function parseAiBillingOverview(input: unknown): AiBillingOverview {
  return billingOverviewSchema.parse(input);
}

/** Parses one developer's billing month at the HTTP boundary. */
export function parseAiDeveloperMonth(input: unknown): AiDeveloperMonth {
  return developerMonthSchema.parse(input);
}

/** Parses a month's data completeness at the HTTP boundary. */
export function parseAiBillingCompleteness(
  input: unknown,
): AiBillingCompleteness {
  return completenessSchema.parse(input);
}

/** Parses the admin list of every user. */
export function parseAiBillingDevelopers(
  input: unknown,
): readonly AiBillingDeveloper[] {
  return z.array(developerSchema).parse(input);
}

/** Parses unmapped project keys at the HTTP boundary. */
export function parseAiUnmappedProjectKeys(
  input: unknown,
): readonly AiUnmappedProjectKey[] {
  return z.array(unmappedKeySchema).parse(input);
}
