import { z } from "zod";
import { aiProviderSchema } from "./ai-usage";

/*
 * Admin-only AI adoption: how much AI each person and the company use. Days
 * are local days in `timeZone`, `YYYY-MM-DD`, at most one per person and tool:
 * never hours or sessions. Code completions are left out.
 *
 * `costUsd` is an API-equivalent estimate of the priced records, except for
 * usage a metered payer (GitHub Copilot) pays for from `credits.from` on,
 * which costs the credits the payer charged the person that UTC day. While
 * `unpricedRecords` is positive, `costUsd` is a known subtotal. `companyDays`
 * are credits no person carries. Credits after `credits.completeThrough` may
 * still grow.
 */

const daySchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const countSchema = z.number().int().nonnegative();
const usdSchema = z.number().nonnegative();

const seatSchema = z
  .object({ provider: aiProviderSchema, since: daySchema })
  .strict();

const personSchema = z
  .object({
    userId: z.number().int(),
    fullName: z.string(),
    /** Linked to a time-tracking user: listed even without usage. */
    linked: z.boolean(),
    /** The local day their first machine first synced usage. */
    firstSyncedOn: daySchema.nullable(),
    seats: z.array(seatSchema),
  })
  .strict();

const daySchemaOf = z
  .object({
    userId: z.number().int(),
    day: daySchema,
    tool: aiProviderSchema,
    costUsd: usdSchema,
    unpricedRecords: countSchema,
    records: countSchema,
    /** Every token read and written. */
    tokens: countSchema,
  })
  .strict();

const companyDaySchema = z
  .object({ day: daySchema, tool: aiProviderSchema, costUsd: usdSchema })
  .strict();

const adoptionSchema = z
  .object({
    today: daySchema,
    timeZone: z.string().min(1),
    from: daySchema,
    to: daySchema,
    /** The bill's currency, for showing costs. */
    currency: z.string().length(3),
    credits: z
      .object({
        provider: aiProviderSchema,
        from: daySchema,
        completeThrough: daySchema.nullable(),
      })
      .strict()
      .nullable(),
    people: z.array(personSchema),
    days: z.array(daySchemaOf),
    companyDays: z.array(companyDaySchema),
  })
  .strict();

const modelsSchema = z
  .object({
    from: daySchema,
    to: daySchema,
    models: z.array(
      z
        .object({
          tool: aiProviderSchema,
          model: z.string(),
          records: countSchema,
          tokens: countSchema,
        })
        .strict(),
    ),
  })
  .strict();

export type AiAdoption = z.infer<typeof adoptionSchema>;
export type AiAdoptionPerson = z.infer<typeof personSchema>;
export type AiAdoptionDay = z.infer<typeof daySchemaOf>;
export type AiAdoptionCompanyDay = z.infer<typeof companyDaySchema>;
export type AiAdoptionModels = z.infer<typeof modelsSchema>;

export function parseAiAdoption(raw: unknown): AiAdoption {
  return adoptionSchema.parse(raw);
}

export function parseAiAdoptionModels(raw: unknown): AiAdoptionModels {
  return modelsSchema.parse(raw);
}
