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

/** A repository's code review credits, mapped to a project. */
const repositoryLine = {
  ...meteredLine,
  subject: { kind: "repository", provider: "copilot", name: "widget-api" },
  metered: {
    ...meteredLine.metered,
    kind: "repository",
    credits: "12.00",
    account: null,
  },
  billableAmount: "0.12",
  convertedAmount: "1.26",
  usage: nothing,
};

/** A repository's code review credits without a repository mapping. */
const unmappedRepositoryLine = {
  ...repositoryLine,
  subject: { ...repositoryLine.subject, name: "gadget-web" },
  project: null,
  unassigned: "unmappedRepository",
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
        adjustments: "0.00",
        invoiced: "20.48",
      },
      adjustments: [],
    },
    exchangeRates: [],
    missingRates: [],
    pendingRates: [],
    adjustedPendingRates: [],
    metered: {
      provider: "copilot",
      provisional: true,
      daysNotImported: 2,
      lowRatioGroups: 1,
      unbilled,
    },
    adjustmentLines: [],
    personalUsage: [],
    adjustedMonths: [],
    close: null,
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
      repositoryLine,
      unmappedRepositoryLine,
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
    {
      name: "an unmapped repository's kind on a developer's line",
      line: { ...unattributedLine, unassigned: "unmappedRepository" },
    },
    {
      name: "an unmapped repository's kind on usage of an unmapped key",
      line: { ...apiLine, project: null, unassigned: "unmappedRepository" },
    },
    {
      name: "a repository without a project marked as an unmapped key",
      line: { ...unmappedRepositoryLine, unassigned: "unmapped" },
    },
    {
      name: "a repository without a project or an Unassigned kind",
      line: { ...unmappedRepositoryLine, unassigned: null },
    },
    {
      name: "a mapped repository marked Unassigned",
      line: { ...repositoryLine, unassigned: "unmappedRepository" },
    },
    {
      name: "a repository naming an account",
      line: {
        ...repositoryLine,
        metered: {
          ...repositoryLine.metered,
          account: meteredLine.metered.account,
        },
      },
    },
    {
      name: "the old lowercase kind",
      line: { ...unmappedRepositoryLine, unassigned: "unmappedrepository" },
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
  const rest = without(
    overview([meteredLine]),
    "developers",
    "personalUsage",
    "adjustedMonths",
  );
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

describe("adjustment lines and closes", () => {
  const clientB = {
    projectId: "102",
    projectName: "Example Client - Gadget Web",
  };
  /** September's API line moved off Widget API after September closed. */
  const credit = {
    adjustsMonth: "2026-09",
    snapshotId: 7,
    subject: ada,
    provider: "claude",
    project: apiLine.project,
    unassigned: null,
    kind: "api",
    meteredKind: null,
    subscriptionId: null,
    plan: null,
    billableAmount: "-12.50",
    billableCurrency: "USD",
    convertedAmount: "-131.25",
    exchangeRate: "10.50",
    missingRate: false,
    credits: null,
    sekOnly: false,
    rateChanged: false,
  };
  const debit = {
    ...credit,
    project: clientB,
    billableAmount: "12.50",
    convertedAmount: "131.25",
  };
  /** September's USD rate overridden after it closed. */
  const sekOnly = {
    ...credit,
    billableAmount: "0.00",
    convertedAmount: "6.25",
    exchangeRate: "11.00",
    sekOnly: true,
    rateChanged: true,
  };
  /** SEK rounding moved at the same rate: no rate change. */
  const rounding = {
    ...credit,
    billableAmount: "0.00",
    convertedAmount: "-0.01",
    sekOnly: true,
  };
  const repository = {
    ...credit,
    subject: { kind: "repository", provider: "copilot", name: "widget-api" },
    provider: "copilot",
    kind: "metered",
    meteredKind: "repository",
    billableAmount: "-0.12",
    convertedAmount: "-1.26",
    credits: "-12.00",
  };
  const personal = {
    ...credit,
    provider: "copilot",
    project: null,
    kind: "metered",
    meteredKind: "personal",
    billableAmount: "0.05",
    convertedAmount: "0.53",
    credits: "5.00",
  };
  const share = {
    ...credit,
    provider: "codex",
    kind: "subscription",
    subscriptionId: 4,
    plan: "ChatGPT Pro",
    billableAmount: "-50.00",
    billableCurrency: "SEK",
    convertedAmount: "-50.00",
    exchangeRate: null,
  };
  const overhead = {
    ...share,
    kind: "overhead",
    project: null,
    billableAmount: "50.00",
    convertedAmount: "50.00",
  };
  const noRate = {
    ...credit,
    billableCurrency: "EUR",
    convertedAmount: null,
    exchangeRate: null,
    missingRate: true,
  };
  const close = {
    closeId: 9,
    closedAt: "2026-11-03T09:15:00.123456Z",
    closedBy: "Grace Hopper",
    adjustmentsPostOn: "2026-11",
    currentExchangeRates: [],
  };

  /** October, closed, with the adjustments to September it froze. */
  function closedOctober(adjustmentLines) {
    const base = overview([apiLine]);
    return {
      ...base,
      adjustmentLines,
      adjustedMonths: [{ month: "2026-09", snapshotId: 7 }],
      totals: {
        ...base.totals,
        adjustments: [
          { currency: "EUR", amount: "-12.50" },
          { currency: "SEK", amount: "0.00" },
          { currency: "USD", amount: "-0.07" },
        ],
        converted: {
          ...base.totals.converted,
          adjustments: null,
          invoiced: null,
        },
      },
      close,
    };
  }

  test("parses signed adjustment lines of every kind beside the month's own lines", () => {
    const input = closedOctober([
      credit,
      debit,
      sekOnly,
      rounding,
      repository,
      personal,
      share,
      overhead,
      noRate,
    ]);

    expect(parseAiBillingOverview(input)).toEqual(input);
  });

  test("parses an open month's negative invoiced total and adjusted months without lines", () => {
    const base = overview([apiLine]);
    const input = {
      ...base,
      adjustmentLines: [credit],
      adjustedMonths: [
        { month: "2026-08", snapshotId: 6 },
        { month: "2026-09", snapshotId: 7 },
      ],
      adjustedPendingRates: [{ month: "2026-09", currency: "USD" }],
      totals: {
        ...base.totals,
        adjustments: [{ currency: "USD", amount: "-12.50" }],
        converted: {
          ...base.totals.converted,
          adjustments: "-131.25",
          invoiced: "-110.77",
        },
      },
    };

    expect(parseAiBillingOverview(input).totals.converted.invoiced).toBe(
      "-110.77",
    );
    expect(parseAiBillingOverview(input).adjustedPendingRates).toEqual([
      { month: "2026-09", currency: "USD" },
    ]);
  });

  test.each([
    {
      name: "a negative amount on the month's own line",
      input: overview([{ ...apiLine, billableAmount: "-0.30" }]),
    },
    {
      name: "a negative converted amount on the month's own line",
      input: overview([{ ...apiLine, convertedAmount: "-3.15" }]),
    },
    {
      name: "a signed zero",
      input: closedOctober([{ ...credit, billableAmount: "-0.00" }]),
    },
    {
      name: "a metered kind on an API adjustment",
      input: closedOctober([{ ...credit, meteredKind: "project" }]),
    },
    {
      name: "a metered adjustment without its kind",
      input: closedOctober([{ ...repository, meteredKind: null }]),
    },
    {
      name: "a metered adjustment without credits",
      input: closedOctober([{ ...repository, credits: null }]),
    },
    {
      name: "credits on an API adjustment",
      input: closedOctober([{ ...credit, credits: "-12.00" }]),
    },
    {
      name: "a subscription adjustment without its subscription",
      input: closedOctober([{ ...share, subscriptionId: null }]),
    },
    {
      name: "a subscription on an API adjustment",
      input: closedOctober([{ ...credit, subscriptionId: 4 }]),
    },
    {
      name: "a plan on an API adjustment",
      input: closedOctober([{ ...credit, plan: "ChatGPT Pro" }]),
    },
    {
      name: "overhead with a project",
      input: closedOctober([{ ...overhead, project: apiLine.project }]),
    },
    {
      name: "Spinit's credits with a project",
      input: closedOctober([{ ...personal, project: apiLine.project }]),
    },
    {
      name: "an API adjustment without a project or an Unassigned kind",
      input: closedOctober([{ ...credit, project: null }]),
    },
    {
      name: "a repository without a project marked as an unmapped key",
      input: closedOctober([
        { ...repository, project: null, unassigned: "unmapped" },
      ]),
    },
    {
      name: "an account's subject on an API adjustment",
      input: closedOctober([{ ...credit, subject: externalLine.subject }]),
    },
    {
      name: "a developer's subject on a repository adjustment",
      input: closedOctober([{ ...repository, subject: ada }]),
    },
    {
      name: "only SEK changed without saying so",
      input: closedOctober([{ ...sekOnly, sekOnly: false }]),
    },
    {
      name: "SEK only while the amount changed",
      input: closedOctober([{ ...credit, sekOnly: true }]),
    },
    {
      name: "a changed rate on an adjustment without one",
      input: closedOctober([{ ...noRate, rateChanged: true }]),
    },
    {
      name: "an adjustment that does not say whether its rate changed",
      input: closedOctober([
        Object.fromEntries(
          Object.entries(credit).filter(([key]) => key !== "rateChanged"),
        ),
      ]),
    },
    {
      name: "a converted amount without a rate",
      input: closedOctober([{ ...noRate, convertedAmount: "-131.25" }]),
    },
    {
      name: "a missing converted amount with a rate",
      input: closedOctober([{ ...noRate, missingRate: false }]),
    },
    {
      name: "an unknown key on an adjustment",
      input: closedOctober([{ ...credit, usage }]),
    },
    {
      name: "an adjustment without its snapshot",
      input: closedOctober([without(credit, "snapshotId")]),
    },
    {
      name: "totals without what the month invoices",
      input: {
        ...closedOctober([]),
        totals: {
          ...closedOctober([]).totals,
          converted: without(closedOctober([]).totals.converted, "invoiced"),
        },
      },
    },
    {
      name: "a close without its current rates",
      input: {
        ...closedOctober([]),
        close: without(close, "currentExchangeRates"),
      },
    },
    {
      name: "an adjusted month without its snapshot",
      input: { ...closedOctober([]), adjustedMonths: [{ month: "2026-09" }] },
    },
    {
      name: "an overview without its close",
      input: without(overview([apiLine]), "close"),
    },
    {
      name: "a corrected month's pending rate without its month",
      input: {
        ...overview([apiLine]),
        adjustedPendingRates: [{ currency: "USD" }],
      },
    },
    {
      name: "a bill that does not say which corrected months' rates are pending",
      input: without(overview([apiLine]), "adjustedPendingRates"),
    },
  ])("rejects $name", ({ input }) => {
    expect(() => parseAiBillingOverview(input)).toThrow();
  });

  test("a closed developer month carries the developer's adjustment lines", () => {
    const rest = without(
      closedOctober([credit, debit]),
      "developers",
      "personalUsage",
      "adjustedMonths",
    );
    const input = {
      ...rest,
      developer: {
        userId: 3,
        fullName: "Ada Lovelace",
        email: "ada@example.com",
      },
      machines: [],
      dailyUsage: [],
      personalUsage: null,
    };

    expect(parseAiDeveloperMonth(input)).toEqual(input);
    expect(() =>
      parseAiDeveloperMonth({ ...input, adjustedMonths: [] }),
    ).toThrow();
  });
});
