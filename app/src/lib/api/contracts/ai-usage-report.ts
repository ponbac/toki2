import { z } from "zod";
import { AI_PROVIDERS, type AiProvider } from "./ai-usage";

const aiProviderSchema = z.enum(AI_PROVIDERS);

/** A calendar date in the server's AI usage time zone, `YYYY-MM-DD`. */
const localDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const instantSchema = z.string().datetime({ offset: true });
const countSchema = z.number().int().nonnegative();
const usdSchema = z.number().nonnegative().finite();

/**
 * An API-equivalent cost estimate in USD. It is `estimated` only when every
 * request was priced; otherwise the cost is unknown and only the known
 * subtotal of the priced requests is available. Never read `partial` as a
 * total: the unpriced requests are not free.
 */
export type AiCost =
  | Readonly<{ kind: "estimated"; usd: number }>
  | Readonly<{ kind: "partial"; knownUsd: number; unpricedRecords: number }>;

const aiUsageTotalsSchema = z
  .object({
    tokens: z
      .object({
        input: countSchema,
        cacheRead: countSchema,
        cacheWrite: countSchema,
        output: countSchema,
      })
      .strict(),
    records: countSchema,
    estimatedCostUsd: usdSchema.nullable(),
    pricedCostUsd: usdSchema,
    unpricedRecords: countSchema,
  })
  .strict()
  .superRefine((totals, context) => {
    if ((totals.estimatedCostUsd === null) !== totals.unpricedRecords > 0) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          "estimatedCostUsd must be null exactly when requests are unpriced",
      });
    }
  })
  .transform(
    ({ tokens, records, estimatedCostUsd, pricedCostUsd, unpricedRecords }) => {
      const cost: AiCost =
        estimatedCostUsd === null
          ? { kind: "partial", knownUsd: pricedCostUsd, unpricedRecords }
          : { kind: "estimated", usd: estimatedCostUsd };
      return { tokens, records, cost };
    },
  );

/** Tokens in four disjoint categories; reasoning is inside `output`. */
export type AiTokenCounts = Readonly<{
  input: number;
  cacheRead: number;
  cacheWrite: number;
  output: number;
}>;

/** Usage summed over some buckets: tokens, requests and the cost estimate. */
export type AiUsageTotals = Readonly<{
  tokens: AiTokenCounts;
  records: number;
  cost: AiCost;
}>;

const projectSchema = z
  .object({ projectId: z.string().min(1), projectName: z.string() })
  .strict();

/** A time-tracking project that AI usage can count for. */
export type AiUsageProject = Readonly<z.infer<typeof projectSchema>>;

const projectKeySchema = z.string().min(1);

/**
 * How a project key's usage is attributed, as a union over `status` with the
 * fields of `extra` on every variant. Only `mapped` usage counts for a
 * project; the rest is unassigned. Only an `unmapped` key can be `mappable`.
 */
function attributionSchema<Extra extends z.ZodRawShape>(extra: Extra) {
  // A plain union: a discriminated union cannot see `status` through a generic
  // extension. Each variant is strict, so exactly one matches.
  return z.union([
    z
      .object({
        projectKey: projectKeySchema,
        status: z.literal("mapped"),
        project: projectSchema,
        mappable: z.literal(false),
      })
      .extend(extra)
      .strict(),
    z
      .object({
        projectKey: projectKeySchema,
        status: z.literal("stale"),
        project: z.null(),
        mappable: z.literal(false),
      })
      .extend(extra)
      .strict(),
    z
      .object({
        projectKey: projectKeySchema,
        status: z.literal("unconfigured"),
        project: z.null(),
        mappable: z.literal(false),
      })
      .extend(extra)
      .strict(),
    z
      .object({
        projectKey: projectKeySchema,
        status: z.literal("unmapped"),
        project: z.null(),
        mappable: z.boolean(),
      })
      .extend(extra)
      .strict(),
    z
      .object({
        projectKey: projectKeySchema,
        status: z.literal("unattributed"),
        project: z.null(),
        mappable: z.literal(false),
      })
      .extend(extra)
      .strict(),
  ]);
}

const plainAttributionSchema = attributionSchema({});

/**
 * A project key and how its usage is attributed: `mapped` to a project,
 * `stale` (mapped outside the configured company, so only an admin can fix
 * it), `unconfigured` (mapped, but time tracking is not configured on the
 * server, which no mapping fixes), `unmapped`, or `unattributed` (no project
 * metadata; never mappable).
 */
export type AiProjectAttribution = Readonly<
  z.infer<typeof plainAttributionSchema>
>;

const reportSchema = z
  .object({
    timeZone: z.string().min(1),
    today: localDateSchema,
    from: localDateSchema,
    to: localDateSchema,
    timeTrackingConfigured: z.boolean(),
    options: z
      .object({
        providers: z.array(aiProviderSchema),
        models: z.array(z.string()),
        projects: z.array(projectSchema),
        unassigned: z.boolean(),
        machineIds: z.array(z.string().uuid()),
      })
      .strict(),
    totals: aiUsageTotalsSchema,
    days: z.array(
      z
        .object({
          date: localDateSchema,
          provider: aiProviderSchema,
          model: z.string(),
          totals: aiUsageTotalsSchema,
        })
        .strict(),
    ),
    projects: z.array(
      z
        .object({
          project: projectSchema.nullable(),
          totals: aiUsageTotalsSchema,
          keys: z.array(attributionSchema({ totals: aiUsageTotalsSchema })),
        })
        .strict(),
    ),
    models: z.array(
      z
        .object({
          provider: aiProviderSchema,
          model: z.string(),
          totals: aiUsageTotalsSchema,
        })
        .strict(),
    ),
    machines: z.array(
      z
        .object({ machineId: z.string().uuid(), totals: aiUsageTotalsSchema })
        .strict(),
    ),
    hours: z.array(
      z
        .object({
          weekday: z.number().int().min(1).max(7),
          hour: z.number().int().min(0).max(23),
          totals: aiUsageTotalsSchema,
        })
        .strict(),
    ),
  })
  .strict();

/**
 * The current user's usage in a range of local days in `timeZone`, narrowed
 * by the filters. `options` lists what the filters can select, whichever are
 * applied. Days and hours without usage are absent. A project entry with a
 * null `project` holds all unassigned usage.
 */
export type AiUsageReport = Readonly<z.infer<typeof reportSchema>>;

const sessionSchema = z
  .object({
    sessionKey: z.string().min(1),
    machineId: z.string().uuid(),
    provider: aiProviderSchema,
    projects: z.array(plainAttributionSchema),
    models: z.array(z.string()),
    firstActiveHour: instantSchema,
    lastActiveHour: instantSchema,
    totals: aiUsageTotalsSchema,
  })
  .strict();

const sessionListSchema = z
  .object({
    timeZone: z.string().min(1),
    from: localDateSchema,
    to: localDateSchema,
    total: countSchema,
    sessions: z.array(sessionSchema),
    nextCursor: z.string().min(1).nullable(),
  })
  .strict();

/**
 * One session of one tool on one machine, counting only its usage inside the
 * range that passes the filters. Active hours are UTC instants of hour starts.
 */
export type AiUsageSession = Readonly<z.infer<typeof sessionSchema>>;

/** The first sessions of a range in the requested order, and how many exist. */
export type AiUsageSessionList = Readonly<z.infer<typeof sessionListSchema>>;

const coverageStatusSchema = z.enum(["ok", "partial", "failed", "missing"]);

const machineSchema = z
  .object({
    machineId: z.string().uuid(),
    label: z.string().min(1),
    clientVersion: z.string().min(1),
    timeZone: z.string().min(1),
    firstSeenAt: instantSchema,
    lastSyncedAt: instantSchema,
    stale: z.boolean(),
    usage: aiUsageTotalsSchema.nullable(),
    coverage: z.array(
      z
        .object({
          provider: aiProviderSchema,
          status: coverageStatusSchema,
          files: countSchema,
          unreadable: countSchema,
          malformedLines: countSchema,
          skippedRecords: countSchema,
          duplicates: countSchema,
          windowStart: instantSchema,
          windowEnd: instantSchema,
          reportedAt: instantSchema,
          pricing: z
            .object({
              status: z.enum(["fresh", "cached", "unavailable", "custom"]),
              fetchedAt: instantSchema.nullable(),
              source: z.string(),
            })
            .strict()
            .nullable(),
          hasUsage: z.boolean(),
        })
        .strict(),
    ),
  })
  .strict();

const machineListSchema = z
  .object({
    staleAfterDays: z.number().int().positive(),
    from: localDateSchema,
    to: localDateSchema,
    machines: z.array(machineSchema),
  })
  .strict();

/** How completely a machine could read one provider's history last time. */
export type AiCoverageStatus = z.infer<typeof coverageStatusSchema>;

/** A machine that uploads the current user's usage, and its last report. */
export type AiUsageMachine = Readonly<z.infer<typeof machineSchema>>;

/**
 * The current user's machines, most recently synced first, each with all its
 * usage from `from` to `to`, which no report filter narrows.
 */
export type AiUsageMachineList = Readonly<z.infer<typeof machineListSchema>>;

const projectKeyListSchema = z
  .object({
    timeTrackingConfigured: z.boolean(),
    projectKeys: z.array(attributionSchema({ lastUsedOn: localDateSchema })),
  })
  .strict();

/**
 * Every project key in the current user's usage, most recently used first.
 * Without time tracking configured, nothing resolves or can be mapped.
 */
export type AiUsageProjectKeyList = Readonly<
  z.infer<typeof projectKeyListSchema>
>;

const mappingProjectListSchema = z.array(projectSchema);

const mappingSchema = z
  .object({
    projectKey: projectKeySchema,
    project: projectSchema,
    status: z.enum(["resolves", "stale", "unconfigured"]),
    stale: z.boolean(),
    inUse: z.boolean(),
    createdBy: z
      .object({ userId: z.number().int(), fullName: z.string() })
      .strict()
      .nullable(),
    createdAt: instantSchema,
    updatedBy: z
      .object({ userId: z.number().int(), fullName: z.string() })
      .strict()
      .nullable(),
    updatedAt: instantSchema,
  })
  .strict();

/** A saved mapping from a project key to a time-tracking project. */
export type AiProjectMapping = Readonly<z.infer<typeof mappingSchema>>;

/** The order of a session listing: most recent, costliest or most tokens first. */
export type AiSessionSort = "recent" | "cost" | "tokens";

/**
 * The range and filters of a report or session listing. An absent bound
 * defaults on the server: `to` to the end of the current month in its time
 * zone, `from` to the start of `to`'s month. `projectId` and `unassigned` are
 * exclusive; an empty `model` selects usage whose model is unknown.
 */
export type AiUsageReportParams = Readonly<{
  from?: string;
  to?: string;
  provider?: AiProvider;
  model?: string;
  projectId?: string;
  unassigned?: true;
  machineId?: string;
}>;

/** Parses a usage report at the HTTP boundary. */
export function parseAiUsageReport(input: unknown): AiUsageReport {
  return reportSchema.parse(input);
}

/** Parses a session listing at the HTTP boundary. */
export function parseAiUsageSessionList(input: unknown): AiUsageSessionList {
  return sessionListSchema.parse(input);
}

/** Parses the machine listing at the HTTP boundary. */
export function parseAiUsageMachineList(input: unknown): AiUsageMachineList {
  return machineListSchema.parse(input);
}

/** Parses the project key listing at the HTTP boundary. */
export function parseAiUsageProjectKeyList(
  input: unknown,
): AiUsageProjectKeyList {
  return projectKeyListSchema.parse(input);
}

/** Parses the projects a key can be mapped to at the HTTP boundary. */
export function parseAiMappingProjectList(
  input: unknown,
): ReadonlyArray<AiUsageProject> {
  return mappingProjectListSchema.parse(input);
}

/** Parses a saved project mapping at the HTTP boundary. */
export function parseAiProjectMapping(input: unknown): AiProjectMapping {
  return mappingSchema.parse(input);
}
