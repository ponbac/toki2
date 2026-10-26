import { z } from "zod";
import { AI_PROVIDERS } from "./ai-usage";

/*
 * Admin-only AI usage contracts: monthly billing, data completeness, and the
 * team-wide project-key mappings. Billing responses are never finer than one
 * local day per developer; they carry no hours or sessions. From the metered
 * cutover on, a metered payer's (GitHub Copilot's) lines bill its own credits
 * per UTC month instead, and a line's subject can be an account no user owns.
 * A month an admin closed is served from its snapshot (`close`); later changes
 * to it post on the next open month as adjustment lines, kept apart from
 * `lines`, with signed amounts.
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
/** A signed exact amount with two decimals, such as `-12.50`: adjustment
 * lines and their totals only, where a credit is negative. A zero is never
 * signed. */
const signedDecimalSchema = z
  .string()
  .regex(/^-?\d+\.\d{2}$/)
  .refine((value) => !/^-0+\.00$/.test(value), {
    message: "a zero is never signed",
  });
/** An instant, RFC 3339 with an offset. */
const instantSchema = z.string().datetime({ offset: true });
const currencySchema = z.string().regex(/^[A-Z]{3}$/);
/** An exchange rate, units of the billing currency per unit, such as `10.50`
 * or `0.064321`. */
const rateSchema = z.string().regex(/^\d+\.\d{2,6}$/);

const projectSchema = z
  .object({ projectId: z.string().min(1), projectName: z.string() })
  .strict();

const developerSchema = z
  .object({ userId: id, fullName: z.string(), email: z.string() })
  .strict();

/** Summed usage; reused by the provider-account contracts. */
export const usageSchema = z
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

/** Why usage is Unassigned, in bill order: repositories nobody mapped to a
 * project (or mapped outside the configured company); a metered payer's code
 * review credits on a repository without a repository mapping to the
 * configured company, which mapping the repository assigns and which holds
 * back month close; or usage outside any repository, which no mapping can
 * assign. */
export const UNASSIGNED_KINDS = [
  "unmapped",
  "unmappedRepository",
  "unattributed",
] as const;
export type UnassignedKind = (typeof UNASSIGNED_KINDS)[number];

/** Whom a line charges for: a developer, or for a metered payer an account
 * no user owns on the billed days, the accounts Toki never listed (the
 * per-day residual), or a repository. */
const subjectSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("user"), userId: id }).strict(),
  z
    .object({
      kind: z.literal("providerAccount"),
      provider: providerSchema,
      accountId: id,
      login: z.string().min(1),
    })
    .strict(),
  z
    .object({ kind: z.literal("unknownAccount"), provider: providerSchema })
    .strict(),
  z
    .object({
      kind: z.literal("repository"),
      provider: providerSchema,
      name: z.string().min(1),
    })
    .strict(),
]);
export type AiBillingSubject = Readonly<z.infer<typeof subjectSchema>>;

/** What a metered line charges: credits attributed to a project (or a kind
 * of Unassigned), and Spinit's: personal/internal usage, credits Toki did
 * not track (the cloud agent's included), credits of an unlinked or external
 * account, and the unknown account's residual; and a repository's credits. */
export const METERED_LINE_KINDS = [
  "project",
  "personal",
  "untracked",
  "unlinkedAccount",
  "externalAccount",
  "unknownAccount",
  "repository",
] as const;
export type AiMeteredLineKind = (typeof METERED_LINE_KINDS)[number];

/** Why a metered line may still change. */
export const METERED_PROVISIONAL_REASONS = [
  "monthInProgress",
  "importIncomplete",
  "notSettled",
  "oldPricingBasis",
  "provisionalModel",
  "unrecognized",
] as const;
export type AiMeteredProvisionalReason =
  (typeof METERED_PROVISIONAL_REASONS)[number];

/** The subject kind each metered kind charges. */
const METERED_SUBJECTS: Readonly<
  Record<AiMeteredLineKind, AiBillingSubject["kind"]>
> = {
  project: "user",
  personal: "user",
  untracked: "user",
  unlinkedAccount: "providerAccount",
  externalAccount: "providerAccount",
  unknownAccount: "unknownAccount",
  repository: "repository",
};

/** A metered payer's line: its credits and how settled they are. */
const meteredLineSchema = z
  .object({
    kind: z.enum(METERED_LINE_KINDS),
    /** Whole credits as hundredths of a credit, `1235.00`: a credit is
     * $0.01, so these are `billableAmount`'s cents. */
    credits: decimalSchema,
    /** The account billed; null for the unknown account and repositories. */
    account: z
      .object({ accountId: id, login: z.string().min(1) })
      .strict()
      .nullable(),
    sku: z.string().min(1).nullable(),
    /** Exactly when it has reasons. */
    provisional: z.boolean(),
    provisionalReasons: z.array(z.enum(METERED_PROVISIONAL_REASONS)),
    /** A settled day-group with the line's usage was billed below 85% of its
     * tracked usage. */
    lowRatio: z.boolean(),
  })
  .strict()
  .refine(
    (metered) => metered.provisional === metered.provisionalReasons.length > 0,
    { message: "provisional must be set exactly when there are reasons" },
  )
  .refine(
    (metered) =>
      (metered.account === null) ===
      (metered.kind === "unknownAccount" || metered.kind === "repository"),
    {
      message:
        "every metered line but the unknown account's and a repository's names its account",
    },
  );
export type AiMeteredLine = Readonly<z.infer<typeof meteredLineSchema>>;

/** Metered kinds that bill a project, or Unassigned without one. */
const PROJECT_METERED_KINDS: ReadonlySet<AiMeteredLineKind> = new Set([
  "project",
  "repository",
]);

const billingLineSchema = z
  .object({
    subject: subjectSchema,
    /** The provider used; a metered line names its payer. */
    provider: providerSchema,
    project: projectSchema.nullable(),
    /** The kind of Unassigned usage, exactly when the line has no project
     * and bills one: neither overhead nor a metered line that is Spinit's.
     * An Unassigned charge has a line per kind; they split it exactly in
     * each currency, so a line's `convertedAmount` need not be its own
     * `billableAmount` converted. */
    unassigned: z.enum(UNASSIGNED_KINDS).nullable(),
    billingMode: z.enum(["api", "subscription", "metered"]),
    unallocatedOverhead: z.boolean(),
    subscriptionId: id.nullable(),
    /** Exactly on metered lines. */
    metered: meteredLineSchema.nullable(),
    /** What the line bills in `billableCurrency`: a fee share, an API
     * estimate rounded to whole cents, or a metered line's credits in USD;
     * null when its cost is unknown. */
    billableAmount: decimalSchema.nullable(),
    billableCurrency: currencySchema,
    /** What the line bills in the billing currency (SEK); null when its own
     * amount is unknown or its currency has no rate (`missingRate`). */
    convertedAmount: decimalSchema.nullable(),
    /** The rate used; null for amounts already in the billing currency. */
    exchangeRate: rateSchema.nullable(),
    missingRate: z.boolean(),
    /** For a metered line, the tracked usage its credits were split by. */
    usage: usageSchema,
  })
  .strict()
  .refine(
    (line) => (line.metered !== null) === (line.billingMode === "metered"),
    { message: "metered must be set exactly on metered lines" },
  )
  .refine(
    (line) =>
      (line.subscriptionId !== null) === (line.billingMode === "subscription"),
    { message: "subscriptionId must be set exactly on subscription lines" },
  )
  .refine(
    (line) => !line.unallocatedOverhead || line.billingMode === "subscription",
    { message: "only a subscription fee can be unallocated overhead" },
  )
  .refine(
    (line) =>
      line.metered === null
        ? line.subject.kind === "user"
        : METERED_SUBJECTS[line.metered.kind] === line.subject.kind,
    { message: "the subject must match the line's kind" },
  )
  .refine(
    (line) =>
      line.metered === null ||
      PROJECT_METERED_KINDS.has(line.metered.kind) ||
      line.project === null,
    { message: "a metered line that is Spinit's has no project" },
  )
  .refine(
    (line) =>
      (line.unassigned !== null) ===
      (line.project === null &&
        (line.metered === null
          ? !line.unallocatedOverhead
          : PROJECT_METERED_KINDS.has(line.metered.kind))),
    { message: "unassigned must be set exactly on Unassigned usage" },
  )
  .refine(
    (line) =>
      (line.unassigned === "unmappedRepository") ===
      (line.metered?.kind === "repository" && line.project === null),
    {
      message:
        "unmappedRepository must be set exactly on repository lines without a project",
    },
  );

/** What an adjustment line corrects: API usage, a subscription fee share,
 * unallocated overhead, or a metered payer's line (`meteredKind`). */
export const ADJUSTED_KINDS = [
  "api",
  "subscription",
  "overhead",
  "metered",
] as const;
export type AiAdjustedKind = (typeof ADJUSTED_KINDS)[number];

/**
 * A change to a closed month since it closed, for one subject, project, kind,
 * currency and payer, posted on the first open month after it. Amounts are
 * signed: a credit is negative. Its SEK amount is the difference of what is
 * invoiced in SEK at the corrected month's rate, never its amount converted,
 * so a rate change alone moves only `convertedAmount` (`sekOnly` with
 * `rateChanged`), as SEK rounding at the same rate can (`sekOnly` alone).
 */
const adjustmentLineSchema = z
  .object({
    /** The closed month it corrects. */
    adjustsMonth: monthSchema,
    /** That month's close. */
    snapshotId: id,
    subject: subjectSchema,
    /** The provider of what changed; for a subscription's lines, the
     * subscription's provider. */
    provider: providerSchema,
    /** Null for Unassigned usage, overhead and Spinit's metered lines. */
    project: projectSchema.nullable(),
    unassigned: z.enum(UNASSIGNED_KINDS).nullable(),
    kind: z.enum(ADJUSTED_KINDS),
    /** Exactly when `kind` is `metered`. */
    meteredKind: z.enum(METERED_LINE_KINDS).nullable(),
    /** Exactly for subscription and overhead lines. */
    subscriptionId: id.nullable(),
    /** The subscription's plan, while it exists. */
    plan: z.string().min(1).nullable(),
    /** What changed, in `billableCurrency`. */
    billableAmount: signedDecimalSchema,
    billableCurrency: currencySchema,
    /** What changed in the billing currency; null without a rate
     * (`missingRate`). */
    convertedAmount: signedDecimalSchema.nullable(),
    /** The corrected month's rate; null in the billing currency. */
    exchangeRate: rateSchema.nullable(),
    missingRate: z.boolean(),
    /** Metered kinds' credits, signed, as hundredths of a credit. */
    credits: signedDecimalSchema.nullable(),
    /** Only the amount in the billing currency changed: the corrected
     * month's exchange rate did (`rateChanged`), or its amounts in the
     * billing currency round differently at the same rate. */
    sekOnly: z.boolean(),
    /** The corrected month's rate differs from the one this line's subject,
     * project, kind and payer was last invoiced at. */
    rateChanged: z.boolean(),
  })
  .strict()
  .refine((line) => (line.meteredKind !== null) === (line.kind === "metered"), {
    message: "meteredKind must be set exactly on metered adjustments",
  })
  .refine(
    (line) =>
      (line.subscriptionId !== null) ===
      (line.kind === "subscription" || line.kind === "overhead"),
    {
      message:
        "subscriptionId must be set exactly on subscription and overhead adjustments",
    },
  )
  .refine((line) => line.plan === null || line.subscriptionId !== null, {
    message: "only a subscription's adjustment names a plan",
  })
  .refine((line) => (line.credits !== null) === (line.kind === "metered"), {
    message: "credits must be set exactly on metered adjustments",
  })
  .refine(
    (line) =>
      line.meteredKind === null
        ? line.subject.kind === "user"
        : METERED_SUBJECTS[line.meteredKind] === line.subject.kind,
    { message: "the subject must match the adjustment's kind" },
  )
  .refine(
    (line) =>
      line.project === null ||
      (line.kind !== "overhead" &&
        (line.meteredKind === null ||
          PROJECT_METERED_KINDS.has(line.meteredKind))),
    { message: "overhead and Spinit's metered adjustments have no project" },
  )
  .refine(
    (line) =>
      (line.unassigned !== null) ===
      (line.project === null &&
        (line.kind === "api" ||
          line.kind === "subscription" ||
          (line.meteredKind !== null &&
            PROJECT_METERED_KINDS.has(line.meteredKind)))),
    { message: "unassigned must be set exactly on Unassigned adjustments" },
  )
  .refine(
    (line) =>
      (line.unassigned === "unmappedRepository") ===
      (line.meteredKind === "repository" && line.project === null),
    {
      message:
        "unmappedRepository must be set exactly on repository adjustments without a project",
    },
  )
  .refine((line) => (line.convertedAmount === null) === line.missingRate, {
    message: "convertedAmount must be null exactly without a rate",
  })
  .refine(
    (line) =>
      line.sekOnly ===
      (line.billableAmount === "0.00" &&
        line.convertedAmount !== null &&
        line.convertedAmount !== "0.00"),
    {
      message:
        "sekOnly must be set exactly when only the billing currency's amount changed",
    },
  )
  .refine((line) => !line.rateChanged || line.exchangeRate !== null, {
    message: "only an adjustment with a rate has a changed rate",
  });

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
    /** The pro-rated fee in the billing currency, converted once and split
     * like the fee; null without a rate. */
    convertedProratedFee: decimalSchema.nullable(),
    exchangeRate: rateSchema.nullable(),
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
    /** What metered lines bill in USD: their credits at $0.01. */
    meteredBilledUsd: decimalSchema,
    usage: usageSchema,
    /** Sums of the converted lines; null when a line it includes has no rate. */
    converted: z
      .object({
        currency: currencySchema,
        billed: decimalSchema.nullable(),
        fees: decimalSchema.nullable(),
        overhead: decimalSchema.nullable(),
        api: decimalSchema.nullable(),
        /** Metered payers' credits. */
        metered: decimalSchema.nullable(),
        /** The adjustment lines, signed, each at the rate of the month it
         * corrects; null when one has no rate. */
        adjustments: signedDecimalSchema.nullable(),
        /** What the month invoices: `billed` plus `adjustments`. */
        invoiced: signedDecimalSchema.nullable(),
      })
      .strict(),
    /** What the adjustment lines bill per currency, signed, apart from every
     * total above. */
    adjustments: z.array(
      z
        .object({ currency: currencySchema, amount: signedDecimalSchema })
        .strict(),
    ),
  })
  .strict();

/** A month's exchange rate to the billing currency. Dates only. */
const exchangeRateSchema = z
  .object({
    month: monthSchema,
    currency: currencySchema,
    /** What the month bills at: the override's rate, else the fetched one. */
    rate: rateSchema,
    /** `admin` for an override, else the provider's name, such as `riksbank`. */
    source: z.string().min(1),
    /** The rate billed at is an average of the days published so far. */
    provisional: z.boolean(),
    /** The provider's rate, kept beneath an override too. */
    fetched: z
      .object({
        rate: rateSchema,
        source: z.string().min(1),
        provisional: z.boolean(),
        /** The latest day whose daily rate the average includes. */
        observedThrough: localDateSchema,
        fetchedOn: localDateSchema,
      })
      .strict()
      .nullable(),
    override: z
      .object({
        rate: rateSchema,
        overriddenOn: localDateSchema,
        overriddenBy: z.string().nullable(),
      })
      .strict()
      .nullable(),
  })
  .strict();

/** A metered payer's lines as a whole, or one developer's of them. */
const meteredBillSchema = z
  .object({
    provider: providerSchema,
    /** Some metered line may still change, or tracked usage is not
     * imported. */
    provisional: z.boolean(),
    /** Account days whose credits were never imported. */
    daysNotImported: count,
    /** Settled day-groups billed below 85% of their tracked usage. */
    lowRatioGroups: count,
    /** Tracked usage of the payer on no line, by why. */
    unbilled: z
      .object({
        /** On days no disposition links the user to an account. */
        trackedUnlinked: usageSchema,
        /** On days whose credits are not imported. */
        notImported: usageSchema,
        /** Of free models, which the payer does not bill. */
        freeModels: usageSchema,
      })
      .strict(),
  })
  .strict();

/** A bill's lines, subscriptions and totals, with its conversion. */
const billShape = {
  billingCurrency: currencySchema,
  lines: z.array(billingLineSchema),
  subscriptions: z.array(subscriptionMonthSchema),
  totals: totalsSchema,
  exchangeRates: z.array(exchangeRateSchema),
  /** Currencies the bill has non-zero amounts in but no rate for. */
  missingRates: z.array(currencySchema),
  /** Currencies needed by the bill whose fetch is still in progress,
   * including usable provisional rates being refreshed. */
  pendingRates: z.array(currencySchema),
  /** The rates still being fetched of the closed months whose changes post
   * on this month, by month: once one arrives, `adjustmentLines` may change,
   * even for a month without lines yet. */
  adjustedPendingRates: z.array(
    z.object({ month: monthSchema, currency: currencySchema }).strict(),
  ),
  /** Null unless the month bills a metered payer's credits. */
  metered: meteredBillSchema.nullable(),
  /** Changes to closed months since they closed, posted on this month, apart
   * from `lines`: no total but `totals.adjustments` and the converted
   * `adjustments` and `invoiced` includes them. */
  adjustmentLines: z.array(adjustmentLineSchema),
};

/** A closed month whose changes post on an open month, with its close. */
const adjustedMonthSchema = z
  .object({ month: monthSchema, snapshotId: id })
  .strict();

/** A closed month's close, on its overview and developer pages. */
const monthCloseSchema = z
  .object({
    closeId: id,
    closedAt: instantSchema,
    /** Null once the admin who closed it is deleted. */
    closedBy: z.string().nullable(),
    /** Where later changes to the month post, `YYYY-MM`. */
    adjustmentsPostOn: monthSchema,
    /** The month's rates as stored now; `exchangeRates` keeps the ones it
     * closed with. A rate changed after close posts the SEK difference as an
     * adjustment. */
    currentExchangeRates: z.array(exchangeRateSchema),
  })
  .strict();

/** The month, and the calendars it follows: the configured time zone's for
 * every line but a metered payer's, which follow UTC months from its cutover
 * on. Both share the month key and its exchange rate. */
const periodShape = {
  month: monthSchema,
  firstDay: localDateSchema,
  lastDay: localDateSchema,
  timeZone: z.string().min(1),
  /** The metered payer's cutover; null when none is configured. */
  meteredPeriod: z
    .object({
      provider: providerSchema,
      /** The first month billed from its credits. */
      from: monthSchema,
      /** Whether this month's metered lines come from its credits. */
      applies: z.boolean(),
      timeZone: z.literal("UTC"),
    })
    .strict()
    .nullable(),
};

const billingOverviewSchema = z
  .object({
    ...periodShape,
    developers: z.array(developerSchema),
    ...billShape,
    // What the bill leaves out: each developer's usage of the repositories
    // they marked personal or internal, as an estimate only.
    personalUsage: z.array(
      z.object({ userId: id, usage: usageSchema }).strict(),
    ),
    /** The closed months whose changes post on this month, oldest first,
     * with or without adjustment lines. */
    adjustedMonths: z.array(adjustedMonthSchema),
    /** The month's close; null while it is open. A closed month is served
     * from its snapshot. */
    close: monthCloseSchema.nullable(),
  })
  .strict();

const dayUsageSchema = z
  .object({
    /** A local date, or a UTC date on metered rows. */
    day: localDateSchema,
    provider: providerSchema,
    billingProvider: providerSchema.nullable().optional(),
    model: z.string(),
    machineId: z.string().uuid(),
    // Null sums every key no developer has mapped: their names are withheld.
    projectKey: z.string().min(1).nullable(),
    project: projectSchema.nullable(),
    /** At its API estimate, by a subscription, or from a metered payer's
     * credits. */
    billingMode: z.enum(["api", "subscription", "metered"]),
    subscriptionId: id.nullable(),
    usage: usageSchema,
  })
  .strict()
  .refine(
    (row) =>
      (row.subscriptionId !== null) === (row.billingMode === "subscription"),
    { message: "subscriptionId must be set exactly on subscription rows" },
  );

const developerMonthSchema = z
  .object({
    ...periodShape,
    developer: developerSchema,
    ...billShape,
    machines: z.array(
      z.object({ machineId: z.string().uuid(), label: z.string() }).strict(),
    ),
    dailyUsage: z.array(dayUsageSchema),
    // The usage `dailyUsage` and the bill leave out; null without any.
    personalUsage: usageSchema.nullable(),
    /** The month's close; null while it is open. Its bill then comes from
     * the snapshot, its usage live. */
    close: monthCloseSchema.nullable(),
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
/** One charge of a subject's usage of a provider to a project. */
export type AiBillingLine = Readonly<z.infer<typeof billingLineSchema>>;
/** A change to a closed month, posted on an open month's bill. */
export type AiAdjustmentLine = Readonly<z.infer<typeof adjustmentLineSchema>>;
/** A closed month posting on an open month, with its close. */
export type AiAdjustedMonth = Readonly<z.infer<typeof adjustedMonthSchema>>;
/** A closed month's close as its bill shows it. */
export type AiMonthClose = Readonly<z.infer<typeof monthCloseSchema>>;
/** A subscription's pro-rated fee for a month and how it was split. */
export type AiSubscriptionMonth = Readonly<
  z.infer<typeof subscriptionMonthSchema>
>;
export type AiBillingTotals = Readonly<z.infer<typeof totalsSchema>>;
export type AiMeteredBill = Readonly<z.infer<typeof meteredBillSchema>>;
export type AiMeteredPeriod = NonNullable<AiBillingOverview["meteredPeriod"]>;
export type AiExchangeRate = Readonly<z.infer<typeof exchangeRateSchema>>;
export type AiBillingOverview = Readonly<z.infer<typeof billingOverviewSchema>>;
/** A developer's usage on one local day of one provider, model, machine and
 * key; a metered payer's usage on one UTC day. */
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

/** Parses an exchange rate an admin set. */
export function parseAiExchangeRate(input: unknown): AiExchangeRate {
  return exchangeRateSchema.parse(input);
}

/** Parses what resetting a rate left: the fetched rate, or null. */
export function parseAiExchangeRateReset(
  input: unknown,
): AiExchangeRate | null {
  return z
    .object({ exchangeRate: exchangeRateSchema.nullable() })
    .strict()
    .parse(input).exchangeRate;
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
