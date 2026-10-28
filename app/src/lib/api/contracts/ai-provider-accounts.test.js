import { describe, expect, test } from "bun:test";
import { parseProviderAccounts } from "./ai-provider-accounts.ts";

const usage = {
  apiEquivalentUsd: 12.3,
  unpricedRecords: 0,
  records: 40,
  tokens: { input: 1, cacheRead: 2, cacheWrite: 3, output: 4, total: 10 },
};

const disposition = {
  id: 7,
  accountId: 89738831,
  login: "example-login",
  target: { kind: "user", userId: 3294 },
  validFrom: "2026-08-25",
  validTo: null,
  updatedAt: "2026-10-24T12:00:00.123456Z",
  updatedBy: 1,
};

/** The overview as the backend serializes it. */
const overview = {
  provider: "copilot",
  directory: {
    status: "ready",
    message: null,
    namespace: "Example-AB",
    lastAttemptAt: "2026-10-24T06:00:00Z",
    lastSuccessAt: "2026-10-24T06:00:00Z",
    lastError: null,
    seatsListed: 2,
    seatsReported: 2,
  },
  today: "2026-10-24",
  reviewFrom: "2026-09-01",
  accounts: [
    {
      accountId: 89738831,
      login: "example-login",
      profile: {
        name: "Ada Andersson",
        email: null,
        fetchedAt: "2026-10-24T06:00:01Z",
      },
      seats: [
        {
          createdAt: "2025-09-16T10:00:00+02:00",
          firstDay: "2025-09-16",
          pendingCancellationOn: "2026-10-31",
          lastActivityAt: null,
          lastActivityClient: null,
          listed: true,
          lastDay: null,
        },
      ],
      dispositions: [disposition],
      currentDispositionId: 7,
      uncoveredSeatDays: [{ from: "2026-09-01", through: "2026-09-30" }],
      needsLook: true,
      suggestions: [],
      defaultPeriod: { validFrom: "2025-09-16", validTo: "2026-08-24" },
    },
    {
      accountId: 5485546,
      login: "other-login",
      profile: null,
      seats: [],
      dispositions: [],
      currentDispositionId: null,
      uncoveredSeatDays: [],
      needsLook: false,
      suggestions: [
        {
          userId: 291,
          confidence: "verified",
          evidence: ["name", "loginFullName"],
        },
      ],
      defaultPeriod: null,
    },
  ],
  activity: [{ userId: 291, days: 6, lastDay: "2026-10-20", usage }],
  unlinkedUsers: [
    {
      userId: 3294,
      uncoveredDays: 9,
      firstDay: "2026-10-01",
      lastDay: "2026-10-20",
      usage,
      suggestedAccountIds: [89738831],
    },
  ],
};

describe("provider-accounts contract", () => {
  test("accepts the overview the backend serializes", () => {
    expect(parseProviderAccounts(overview)).toEqual(overview);
  });

  test("refuses a current disposition the account doesn't have", () => {
    const [first, second] = overview.accounts;
    expect(() =>
      parseProviderAccounts({
        ...overview,
        accounts: [{ ...first, currentDispositionId: 8 }, second],
      }),
    ).toThrow();
  });
});
