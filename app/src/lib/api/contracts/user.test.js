import { describe, expect, test } from "bun:test";
import { parseOAuthGrants } from "./user.ts";

const grant = {
  id: 7,
  clientName: "Claude",
  clientHost: "claude.ai",
  redirectDestination: "claude.ai",
  createdAt: "2026-10-01T08:00:00Z",
  lastUsedAt: "2026-10-18T09:30:00+02:00",
  expiresAt: "2026-12-30T08:00:00Z",
};

describe("connected-apps contract", () => {
  test("accepts verified and unverified grants", () => {
    const unverified = {
      ...grant,
      id: 8,
      clientName: "Cursor",
      clientHost: null,
      redirectDestination: "cursor://anysphere.cursor-mcp",
    };

    expect(parseOAuthGrants([grant, unverified])).toEqual([grant, unverified]);
  });

  test.each([
    { name: "an unknown field", input: { ...grant, refreshToken: "x" } },
    { name: "a missing host", input: { ...grant, clientHost: undefined } },
    {
      name: "a timestamp without offset",
      input: { ...grant, lastUsedAt: "2026-10-18T09:30:00" },
    },
    { name: "a non-positive id", input: { ...grant, id: 0 } },
  ])("rejects a grant with $name", ({ input }) => {
    expect(() => parseOAuthGrants([input])).toThrow();
  });
});
