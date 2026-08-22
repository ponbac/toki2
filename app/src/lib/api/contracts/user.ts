import { z } from "zod";

const roleSchema = z.enum(["Admin", "User"]);

const userSchema = z
  .object({
    id: z.number().int().positive(),
    email: z.string().email(),
    fullName: z.string(),
    picture: z.string(),
    roles: z.array(roleSchema),
    avatarUrl: z.string().url().nullable(),
  })
  .strict();

/** Closed set of Automation API capabilities a token may hold. */
export const API_TOKEN_CAPABILITIES = [
  "timer:read",
  "catalog:read",
  "entries:read",
  "work-items:read",
  "pull-requests:read",
] as const;

const apiTokenCapabilitySchema = z.enum(API_TOKEN_CAPABILITIES);
const apiTokenCapabilitiesSchema = z.array(apiTokenCapabilitySchema).nonempty();

const apiTokenSchema = z
  .object({
    id: z.number().int().positive(),
    name: z.string().trim().min(1).max(64),
    prefix: z.string().regex(/^toki_[0-9a-f]{7}$/),
    capabilities: apiTokenCapabilitiesSchema,
    createdAt: z.string().datetime({ offset: true }),
  })
  .strict();

const createdApiTokenSchema = apiTokenSchema.extend({
  token: z.string().regex(/^toki_[0-9a-f]{64}$/),
});

/** A role returned for the signed-in Toki user. */
export type Role = z.infer<typeof roleSchema>;

/** Public profile data returned by the current-user endpoint. */
export type User = Readonly<z.infer<typeof userSchema>>;

/** A durable permission that narrows the Automation API for one API token. */
export type ApiTokenCapability = z.infer<typeof apiTokenCapabilitySchema>;

/** Non-empty set of known API token capabilities. */
export type ApiTokenCapabilities = z.infer<typeof apiTokenCapabilitiesSchema>;

/** Revocable metadata for an API token; it never contains the secret. */
export type ApiToken = Readonly<z.infer<typeof apiTokenSchema>>;

/** A newly issued token whose secret is returned exactly once. */
export type CreatedApiToken = Readonly<z.infer<typeof createdApiTokenSchema>>;

/** Parses the current-user response at the HTTP boundary. */
export function parseUser(input: unknown): User {
  return userSchema.parse(input);
}

/** Parses a token metadata list without allowing secret-bearing entries. */
export function parseApiTokens(input: unknown): readonly ApiToken[] {
  return z.array(apiTokenSchema).parse(input);
}

/** Parses the one-time token issuance response at the HTTP boundary. */
export function parseCreatedApiToken(input: unknown): CreatedApiToken {
  return createdApiTokenSchema.parse(input);
}
