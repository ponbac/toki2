import { z } from "zod";
import { usageSchema } from "./ai-admin";
import { AI_PROVIDERS } from "./ai-usage";

/*
 * Admin-only provider accounts: the accounts holding seats at a metered payer
 * (GitHub Copilot), and whose usage each one is. Days are UTC calendar days as
 * `YYYY-MM-DD`, both ends inclusive, as GitHub bills; instants are RFC 3339.
 */

const count = z.number().int().nonnegative();
const id = z.number().int().positive();
/** A UTC calendar day, `YYYY-MM-DD`. */
const utcDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const instantSchema = z.string().datetime({ offset: true });

/** Whose an account's usage is: a Toki user's, or not a Toki user's
 * (Spinit overhead). */
const targetSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("user"), userId: id }).strict(),
  z.object({ kind: z.literal("external") }).strict(),
]);

const dispositionSchema = z
  .object({
    id,
    accountId: id,
    /** The account's login when the disposition was last written. */
    login: z.string().min(1),
    target: targetSchema,
    validFrom: utcDateSchema,
    /** Null while ongoing. */
    validTo: utcDateSchema.nullable(),
    updatedAt: instantSchema,
    updatedBy: id.nullable(),
  })
  .strict();

const directorySchema = z
  .object({
    status: z.enum(["ready", "disabled", "misconfigured"]),
    /** Why the directory is misconfigured; null otherwise. */
    message: z.string().nullable(),
    /** Where the accounts live, such as the GitHub organization. */
    namespace: z.string(),
    lastAttemptAt: instantSchema.nullable(),
    lastSuccessAt: instantSchema.nullable(),
    lastError: z.string().nullable(),
    seatsListed: count.nullable(),
    seatsReported: count.nullable(),
  })
  .strict();

const seatSchema = z
  .object({
    /** The payer's assignment instant. */
    createdAt: instantSchema,
    /** Its UTC day, the seat's first. */
    firstDay: utcDateSchema,
    pendingCancellationOn: utcDateSchema.nullable(),
    lastActivityAt: instantSchema.nullable(),
    lastActivityClient: z.string().nullable(),
    /** Whether the payer still lists the seat. */
    listed: z.boolean(),
    /** The seat's last day, inclusive, once it is no longer listed. */
    lastDay: utcDateSchema.nullable(),
  })
  .strict();

export const SUGGESTION_EVIDENCE = [
  "email",
  "name",
  "nameFirstLast",
  "nameInitial",
  "nameFirstOnly",
  "loginFullName",
  "loginInitialSurname",
  "loginFirstSurnamePrefix",
  "loginFirstOnly",
] as const;
export type AiSuggestionEvidence = (typeof SUGGESTION_EVIDENCE)[number];

const suggestionSchema = z
  .object({
    userId: id,
    confidence: z.enum(["verified", "probable"]),
    evidence: z.array(z.enum(SUGGESTION_EVIDENCE)),
  })
  .strict();

const periodSchema = z
  .object({ validFrom: utcDateSchema, validTo: utcDateSchema.nullable() })
  .strict();

const accountSchema = z
  .object({
    accountId: id,
    login: z.string().min(1),
    profile: z
      .object({
        name: z.string().nullable(),
        /** The public email only. */
        email: z.string().nullable(),
        fetchedAt: instantSchema,
      })
      .strict()
      .nullable(),
    /** Oldest first. */
    seats: z.array(seatSchema),
    /** By first day. */
    dispositions: z.array(dispositionSchema),
    /** The disposition whose period holds today. */
    currentDispositionId: id.nullable(),
    /** Seated days in the review window that no disposition covers. */
    uncoveredSeatDays: z.array(
      z.object({ from: utcDateSchema, through: utcDateSchema }).strict(),
    ),
    needsLook: z.boolean(),
    /** Best first; empty without a default period. */
    suggestions: z.array(suggestionSchema),
    /** The period a new disposition gets by default: the flagged uncovered
     * days first. Null when dispositions cover every seated day, and every
     * later one while a seat is listed. */
    defaultPeriod: periodSchema.nullable(),
  })
  .strict()
  .refine(
    (account) =>
      account.currentDispositionId === null ||
      account.dispositions.some(
        (disposition) => disposition.id === account.currentDispositionId,
      ),
    { message: "currentDispositionId must be one of the account's" },
  );

const activitySchema = z
  .object({
    userId: id,
    /** Days with usage the payer paid for. */
    days: count,
    lastDay: utcDateSchema.nullable(),
    usage: usageSchema,
  })
  .strict();

const unlinkedUserSchema = z
  .object({
    userId: id,
    /** Days with usage that none of the user's dispositions covers. */
    uncoveredDays: count,
    firstDay: utcDateSchema,
    lastDay: utcDateSchema,
    /** The usage on those days. */
    usage: usageSchema,
    /** Accounts whose top suggestion is the user. */
    suggestedAccountIds: z.array(id),
  })
  .strict();

const providerAccountsSchema = z
  .object({
    /** The metered payer whose accounts these are, such as `copilot`. */
    provider: z.enum(AI_PROVIDERS),
    directory: directorySchema,
    today: utcDateSchema,
    /** The first day of the review window, the previous UTC month's first. */
    reviewFrom: utcDateSchema,
    accounts: z.array(accountSchema),
    /** Each user's usage the payer paid for in the review window. */
    activity: z.array(activitySchema),
    /** Users with such usage on days without a link. */
    unlinkedUsers: z.array(unlinkedUserSchema),
  })
  .strict();

const refreshSchema = z
  .object({
    startedAt: instantSchema,
    finishedAt: instantSchema,
    seatsListed: count,
    seatsReported: count.nullable(),
    /** Whether the listing was complete, so seats it omitted were ended. */
    complete: z.boolean(),
    seatsStarted: count,
    seatsEnded: count,
    profilesRead: count,
    profilesFailed: count,
  })
  .strict();

export const SUGGESTION_SKIP_REASONS = [
  "notVerified",
  "changed",
  "alreadyDisposed",
  "accountOverlap",
  "userOverlap",
] as const;
export type AiSuggestionSkipReason = (typeof SUGGESTION_SKIP_REASONS)[number];

const acceptedSuggestionsSchema = z
  .object({
    created: z.array(dispositionSchema),
    skipped: z.array(
      z
        .object({ accountId: id, reason: z.enum(SUGGESTION_SKIP_REASONS) })
        .strict(),
    ),
  })
  .strict();

export type AiAccountTarget = Readonly<z.infer<typeof targetSchema>>;
export type AiAccountDisposition = Readonly<z.infer<typeof dispositionSchema>>;
export type AiAccountDirectory = Readonly<z.infer<typeof directorySchema>>;
export type AiProviderSeat = Readonly<z.infer<typeof seatSchema>>;
export type AiAccountSuggestion = Readonly<z.infer<typeof suggestionSchema>>;
export type AiAccountPeriod = Readonly<z.infer<typeof periodSchema>>;
export type AiProviderAccount = Readonly<z.infer<typeof accountSchema>>;
export type AiMeteredActivity = Readonly<z.infer<typeof activitySchema>>;
export type AiUnlinkedUser = Readonly<z.infer<typeof unlinkedUserSchema>>;
export type AiProviderAccounts = Readonly<
  z.infer<typeof providerAccountsSchema>
>;
export type AiDirectoryRefresh = Readonly<z.infer<typeof refreshSchema>>;
export type AiAcceptedSuggestions = Readonly<
  z.infer<typeof acceptedSuggestionsSchema>
>;

/** A disposition's target and days, as created or replaced. */
export type AiAccountDispositionTerms = Readonly<{
  target: AiAccountTarget;
  validFrom: string;
  validTo: string | null;
}>;

/** Parses the provider-account overview at the HTTP boundary. */
export function parseProviderAccounts(input: unknown): AiProviderAccounts {
  return providerAccountsSchema.parse(input);
}

/** Parses a disposition the server created or updated. */
export function parseAccountDisposition(input: unknown): AiAccountDisposition {
  return dispositionSchema.parse(input);
}

/** Parses what one directory refresh did. */
export function parseDirectoryRefresh(input: unknown): AiDirectoryRefresh {
  return refreshSchema.parse(input);
}

/** Parses the dispositions accepting suggestions created, and the skips. */
export function parseAcceptedSuggestions(
  input: unknown,
): AiAcceptedSuggestions {
  return acceptedSuggestionsSchema.parse(input);
}
