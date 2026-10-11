import { z } from "zod";

const aiProviderSchema = z.enum(["codex", "claude", "grok", "copilot"]);

/** A calendar date in the server's AI usage time zone, `YYYY-MM-DD`. */
const localDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);

/** The longest plan name, in Unicode code points, as the server counts. */
export const MAX_PLAN_CODE_POINTS = 64;

/** Counts Unicode code points, not UTF-16 code units as `.length` does. */
export function codePointLength(text: string): number {
  return Array.from(text).length;
}

const aiSubscriptionSchema = z
  .object({
    id: z.number().int().positive(),
    userId: z.number().int().positive(),
    provider: aiProviderSchema,
    plan: z
      .string()
      .min(1)
      .refine((plan) => codePointLength(plan) <= MAX_PLAN_CODE_POINTS),
    monthlyCost: z.string().regex(/^\d{1,10}\.\d{2}$/),
    currency: z.string().regex(/^[A-Z]{3}$/),
    validFrom: localDateSchema,
    validTo: localDateSchema.nullable(),
  })
  .strict();

const aiSubscriptionListSchema = z
  .object({
    timeZone: z.string().min(1),
    today: localDateSchema,
    subscriptions: z.array(aiSubscriptionSchema),
  })
  .strict();

const aiSubscriptionMismatchSchema = z
  .object({
    userId: z.number().int().positive(),
    provider: aiProviderSchema,
    firstDay: localDateSchema,
    lastDay: localDateSchema,
    days: z.number().int().positive(),
    plans: z.array(z.string().min(1)).min(1),
    lastUncoveredDay: localDateSchema.nullable(),
  })
  .strict();

const aiSubscriptionMismatchListSchema = z
  .object({
    from: localDateSchema,
    to: localDateSchema,
    mismatches: z.array(aiSubscriptionMismatchSchema),
  })
  .strict();

/** A tool whose usage token-ledger uploads. */
export type AiProvider = z.infer<typeof aiProviderSchema>;

/** Every provider, in display order. */
export const AI_PROVIDERS = aiProviderSchema.options;

/**
 * A declared subscription. The fee is an exact decimal string in its own
 * currency; `validTo` is inclusive and null while the subscription is ongoing.
 */
export type AiSubscription = Readonly<z.infer<typeof aiSubscriptionSchema>>;

/**
 * Subscriptions with the server's AI usage time zone, whose calendar days their
 * dates are, and today's date there.
 */
export type AiSubscriptionList = Readonly<
  z.infer<typeof aiSubscriptionListSchema>
>;

/** The terms a developer declares for a subscription. */
export type AiSubscriptionTerms = Readonly<
  Pick<
    AiSubscription,
    "provider" | "plan" | "monthlyCost" | "currency" | "validFrom" | "validTo"
  >
>;

/**
 * Days on which a provider reported a paid plan, but no subscription to it is
 * declared for them, all between the same declared subscriptions. Declaring
 * one from `firstDay` through `lastUncoveredDay` (or ongoing, when null)
 * overlaps no other subscription.
 */
export type AiSubscriptionMismatch = Readonly<
  z.infer<typeof aiSubscriptionMismatchSchema>
>;

/** Mismatches found between two local days, both inclusive. */
export type AiSubscriptionMismatchList = Readonly<
  z.infer<typeof aiSubscriptionMismatchListSchema>
>;

/** Parses one subscription at the HTTP boundary. */
export function parseAiSubscription(input: unknown): AiSubscription {
  return aiSubscriptionSchema.parse(input);
}

/** Parses a subscription list at the HTTP boundary. */
export function parseAiSubscriptionList(input: unknown): AiSubscriptionList {
  return aiSubscriptionListSchema.parse(input);
}

/** Parses subscription mismatches at the HTTP boundary. */
export function parseAiSubscriptionMismatchList(
  input: unknown,
): AiSubscriptionMismatchList {
  return aiSubscriptionMismatchListSchema.parse(input);
}
