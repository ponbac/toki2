import { z } from "zod";
import { AI_PROVIDERS } from "./ai-usage";

/*
 * Admin-only repository mappings: the repositories a metered payer (GitHub
 * Copilot) billed code review credits to, and the time-tracking projects those
 * credits bill to. Months are UTC months, `YYYY-MM`; days are UTC days,
 * `YYYY-MM-DD`; instants are RFC 3339.
 */

/** A UTC month, `YYYY-MM`. */
const monthSchema = z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/);
/** A UTC calendar day, `YYYY-MM-DD`. */
const utcDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const instantSchema = z.string().datetime({ offset: true });
/** Hundredths of a credit, `1200.00`: a credit is $0.01. */
const creditsSchema = z.string().regex(/^\d+\.\d{2}$/);
/** A bare repository name as the payer spells it: 1 to 200 characters. */
const repositoryNameSchema = z.string().min(1).max(200);

const projectSchema = z
  .object({ projectId: z.string().min(1), projectName: z.string() })
  .strict();

const editorSchema = z
  .object({ userId: z.number().int(), fullName: z.string() })
  .strict();

/** How a mapping resolves: only a resolving one bills the repository's
 * credits to its project; a stale one (another company's project) or one
 * without time tracking configured bills them as Unassigned. */
export const REPOSITORY_MAPPING_STATUSES = [
  "resolves",
  "stale",
  "unconfigured",
] as const;
export type AiRepositoryMappingStatus =
  (typeof REPOSITORY_MAPPING_STATUSES)[number];

const mappingSchema = z
  .object({
    /** As spelled when the mapping was last saved. */
    repository: repositoryNameSchema,
    project: projectSchema,
    status: z.enum(REPOSITORY_MAPPING_STATUSES),
    /** Null once that user is deleted. */
    createdBy: editorSchema.nullable(),
    createdAt: instantSchema,
    /** Null once that user is deleted. */
    updatedBy: editorSchema.nullable(),
    updatedAt: instantSchema,
  })
  .strict();

/** A project of resolving project-key mappings naming the repository, with
 * those keys. */
const suggestionSchema = z
  .object({
    project: projectSchema,
    projectKeys: z.array(z.string().min(1)).min(1),
  })
  .strict();

/** A repository's credits in one of the overview's months. */
const monthCreditsSchema = z
  .object({
    month: monthSchema,
    credits: creditsSchema,
    /** Whether they bill a line, as the bill decides: the month is on or
     * after the cutover and its credits, rounded once, come to at least one
     * credit. Credits before the cutover or under one credit bill nothing. */
    billed: z.boolean(),
  })
  .strict();

const repositorySchema = z
  .object({
    /** The bare name, as the payer last spelled it. */
    repository: repositoryNameSchema,
    /** Credits in each of the overview's months, in order. */
    months: z.array(monthCreditsSchema).min(1),
    /** The latest UTC day the payer billed it; null for a mapping of a
     * repository never billed. */
    lastBilledOn: utcDateSchema.nullable(),
    mapping: mappingSchema.nullable(),
    /** Empty when the mapping resolves. */
    suggestions: z.array(suggestionSchema),
  })
  .strict()
  .refine(
    (row) => row.mapping?.status !== "resolves" || row.suggestions.length === 0,
    { message: "a repository whose mapping resolves has no suggestions" },
  )
  .refine((row) => row.lastBilledOn !== null || row.mapping !== null, {
    message: "a listed repository is billed or mapped",
  });

const overviewSchema = z
  .object({
    /** The metered payer, such as `copilot`. */
    provider: z.enum(AI_PROVIDERS),
    /** The metered cutover, the first month whose bill has repository
     * lines; null without one. */
    billedFrom: monthSchema.nullable(),
    /** The months credits are shown for: the previous and current UTC
     * month, oldest first. */
    months: z.array(monthSchema).min(1),
    /** Those with a billed month that bills as Unassigned first, then by
     * name. */
    repositories: z.array(repositorySchema),
  })
  .strict()
  .refine(
    (overview) =>
      overview.repositories.every(
        (row) =>
          row.months.length === overview.months.length &&
          row.months.every(
            ({ month }, index) => month === overview.months[index],
          ),
      ),
    { message: "every repository's months are the overview's months" },
  )
  .refine(
    ({ billedFrom, repositories }) =>
      repositories.every((row) =>
        row.months.every(
          ({ month, billed }) =>
            !billed || (billedFrom !== null && month >= billedFrom),
        ),
      ),
    { message: "only months from the cutover on bill" },
  );

/** A saved mapping of a repository to a time-tracking project. */
export type AiRepositoryMapping = Readonly<z.infer<typeof mappingSchema>>;
/** A project suggested for a repository by mapped project keys. */
export type AiRepositorySuggestion = Readonly<z.infer<typeof suggestionSchema>>;
/** A repository the payer billed or someone mapped, with its credits. */
export type AiRepositoryMappingRow = Readonly<z.infer<typeof repositorySchema>>;
/** A repository's credits in one month, and whether they bill a line. */
export type AiRepositoryMonthCredits = Readonly<
  z.infer<typeof monthCreditsSchema>
>;
/** Every repository the payer billed and every mapping. */
export type AiRepositoryMappings = Readonly<z.infer<typeof overviewSchema>>;

/** Parses the repository mappings overview at the HTTP boundary. */
export function parseAiRepositoryMappings(
  input: unknown,
): AiRepositoryMappings {
  return overviewSchema.parse(input);
}

/** Parses a saved repository mapping at the HTTP boundary. */
export function parseAiRepositoryMapping(input: unknown): AiRepositoryMapping {
  return mappingSchema.parse(input);
}
