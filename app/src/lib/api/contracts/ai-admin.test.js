import { describe, expect, test } from "bun:test";
import { parseAiBillingOverview, parseAiDeveloperMonth } from "./ai-admin.ts";

const usage = {
  apiEquivalentUsd: 0.7,
  unpricedRecords: 0,
  records: 7,
  tokens: { input: 10, cacheRead: 20, cacheWrite: 5, output: 3, total: 38 },
};

/** `object` without the keys `keys`. */
function without(object, ...keys) {
  return Object.fromEntries(
    Object.entries(object).filter(([key]) => !keys.includes(key)),
  );
}

const nothing = {
  apiEquivalentUsd: 0,
  unpricedRecords: 0,
  records: 0,
  tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, total: 0 },
};

const ada = { kind: "user", userId: 3 };

/** An API line as the backend serializes it. */
const apiLine = {
  subject: ada,
  provider: "codex",
  project: { projectId: "101", projectName: "Example Client - Widget API" },
  unassigned: null,
  billingMode: "api",
  unallocatedOverhead: false,
  subscriptionId: null,
  metered: null,
  billableAmount: "0.30",
  billableCurrency: "USD",
  convertedAmount: "3.15",
  exchangeRate: "10.50",
  missingRate: false,
  usage,
};

/** A metered project line as the backend serializes it. */
const meteredLine = {
  ...apiLine,
  provider: "copilot",
  billingMode: "metered",
  metered: {
    kind: "project",
    credits: "70.00",
    account: { accountId: 1001, login: "ada-lovelace" },
    sku: "Copilot AI Credits",
    provisional: false,
    provisionalReasons: [],
    lowRatio: false,
  },
  billableAmount: "0.70",
  convertedAmount: "7.35",
};

const externalLine = {
  ...meteredLine,
  subject: {
    kind: "providerAccount",
    provider: "copilot",
    accountId: 2002,
    login: "kj-contractor",
  },
  project: null,
  metered: {
    ...meteredLine.metered,
    kind: "externalAccount",
    account: { accountId: 2002, login: "kj-contractor" },
  },
  usage: nothing,
};

const unknownLine = {
  ...meteredLine,
  subject: { kind: "unknownAccount", provider: "copilot" },
  project: null,
  metered: {
    ...meteredLine.metered,
    kind: "unknownAccount",
    credits: "5.00",
    account: null,
    sku: null,
    provisional: true,
    provisionalReasons: ["monthInProgress", "notSettled"],
  },
  billableAmount: "0.05",
  convertedAmount: "0.53",
  usage: nothing,
};

const unattributedLine = {
  ...meteredLine,
  project: null,
  unassigned: "unattributed",
  metered: { ...meteredLine.metered, credits: "20.00" },
  billableAmount: "0.20",
  convertedAmount: "2.10",
};

const untrackedLine = {
  ...meteredLine,
  project: null,
  metered: {
    ...meteredLine.metered,
    kind: "untracked",
    sku: "Copilot Cloud Agent",
  },
};

const unbilled = {
  trackedUnlinked: nothing,
  notImported: usage,
  freeModels: nothing,
};

/** A month's overview as the backend serializes it, with metered lines. */
function overview(lines) {
  return {
    month: "2026-10",
    firstDay: "2026-10-01",
    lastDay: "2026-10-31",
    timeZone: "Europe/Stockholm",
    meteredPeriod: {
      provider: "copilot",
      from: "2026-10",
      applies: true,
      timeZone: "UTC",
    },
    developers: [
      { userId: 3, fullName: "Ada Lovelace", email: "ada@example.com" },
    ],
    billingCurrency: "SEK",
    lines,
    subscriptions: [],
    totals: {
      fees: [],
      apiBilledUsd: "0.30",
      apiUnpricedRecords: 0,
      meteredBilledUsd: "1.65",
      usage,
      converted: {
        currency: "SEK",
        billed: "20.48",
        fees: "0.00",
        overhead: "0.00",
        api: "3.15",
        metered: "17.33",
      },
    },
    exchangeRates: [],
    missingRates: [],
    pendingRates: [],
    metered: {
      provider: "copilot",
      provisional: true,
      daysNotImported: 2,
      lowRatioGroups: 1,
      unbilled,
    },
    personalUsage: [],
  };
}

describe("billing overview contract", () => {
  test("parses metered lines of every subject beside API lines", () => {
    const input = overview([
      apiLine,
      meteredLine,
      unattributedLine,
      untrackedLine,
      externalLine,
      unknownLine,
    ]);

    expect(parseAiBillingOverview(input)).toEqual(input);
  });

  test("parses a month without a cutover", () => {
    const input = {
      ...overview([apiLine]),
      meteredPeriod: null,
      metered: null,
    };

    expect(parseAiBillingOverview(input).meteredPeriod).toBeNull();
  });

  test.each([
    {
      name: "a metered line without its charge",
      line: { ...meteredLine, metered: null },
    },
    {
      name: "an API line with a metered charge",
      line: { ...apiLine, metered: meteredLine.metered },
    },
    {
      name: "an account's line charged to a developer",
      line: { ...externalLine, subject: ada },
    },
    {
      name: "a developer's line charged to an account",
      line: { ...meteredLine, subject: externalLine.subject },
    },
    {
      name: "an account subject on an API line",
      line: { ...apiLine, subject: externalLine.subject },
    },
    {
      name: "Spinit's credits with a project",
      line: { ...untrackedLine, project: apiLine.project },
    },
    {
      name: "Spinit's credits marked Unassigned",
      line: { ...untrackedLine, unassigned: "unattributed" },
    },
    {
      name: "a project's credits without a project or an Unassigned kind",
      line: { ...meteredLine, project: null },
    },
    {
      name: "the unknown account naming an account",
      line: {
        ...unknownLine,
        metered: {
          ...unknownLine.metered,
          account: externalLine.metered.account,
        },
      },
    },
    {
      name: "an account's line without its account",
      line: {
        ...externalLine,
        metered: { ...externalLine.metered, account: null },
      },
    },
    {
      name: "provisional without a reason",
      line: {
        ...meteredLine,
        metered: { ...meteredLine.metered, provisional: true },
      },
    },
    {
      name: "a reason on a settled line",
      line: {
        ...meteredLine,
        metered: {
          ...meteredLine.metered,
          provisionalReasons: ["notSettled"],
        },
      },
    },
    {
      name: "an unknown provisional reason",
      line: {
        ...meteredLine,
        metered: {
          ...meteredLine.metered,
          provisional: true,
          provisionalReasons: ["late"],
        },
      },
    },
    {
      name: "an unknown metered kind",
      line: {
        ...meteredLine,
        metered: { ...meteredLine.metered, kind: "seats" },
      },
    },
    {
      name: "an unknown key on the charge",
      line: {
        ...meteredLine,
        metered: { ...meteredLine.metered, repository: null },
      },
    },
    {
      name: "an unknown key on the subject",
      line: { ...meteredLine, subject: { ...ada, login: "ada-lovelace" } },
    },
    {
      name: "the old userId",
      line: { ...apiLine, userId: 3 },
    },
  ])("rejects $name", ({ line }) => {
    expect(() => parseAiBillingOverview(overview([line]))).toThrow();
  });

  test.each([
    {
      name: "a period without the metered basis",
      input: without(overview([apiLine]), "meteredPeriod"),
    },
    {
      name: "a metered period on another time zone",
      input: {
        ...overview([apiLine]),
        meteredPeriod: {
          provider: "copilot",
          from: "2026-10",
          applies: true,
          timeZone: "Europe/Stockholm",
        },
      },
    },
    {
      name: "totals without the metered amount",
      input: {
        ...overview([apiLine]),
        totals: without(overview([apiLine]).totals, "meteredBilledUsd"),
      },
    },
    {
      name: "an unknown key on the metered bill",
      input: {
        ...overview([apiLine]),
        metered: { ...overview([]).metered, credits: "1.00" },
      },
    },
  ])("rejects $name", ({ input }) => {
    expect(() => parseAiBillingOverview(input)).toThrow();
  });
});

describe("developer month contract", () => {
  const row = {
    day: "2026-10-12",
    provider: "opencode",
    billingProvider: "copilot",
    model: "gpt-6.1-sol",
    machineId: "8d1f6a5e-2a4f-4a53-9a51-1d5b1f8f2c11",
    projectKey: "github.com/example-org/widget-api",
    project: apiLine.project,
    billingMode: "metered",
    subscriptionId: null,
    usage,
  };
  const rest = without(overview([meteredLine]), "developers", "personalUsage");
  const month = (dailyUsage) => ({
    ...rest,
    developer: {
      userId: 3,
      fullName: "Ada Lovelace",
      email: "ada@example.com",
    },
    machines: [],
    dailyUsage,
    personalUsage: null,
  });

  test("parses metered days beside subscription days", () => {
    const input = month([
      row,
      { ...row, billingMode: "subscription", subscriptionId: 4 },
    ]);

    expect(parseAiDeveloperMonth(input)).toEqual(input);
  });

  test.each([
    {
      name: "a metered day with a subscription",
      day: { ...row, subscriptionId: 4 },
    },
    {
      name: "a subscription day without one",
      day: { ...row, billingMode: "subscription" },
    },
    {
      name: "a day without its billing mode",
      day: without(row, "billingMode"),
    },
  ])("rejects $name", ({ day }) => {
    expect(() => parseAiDeveloperMonth(month([day]))).toThrow();
  });
});
