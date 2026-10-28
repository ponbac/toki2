import { describe, expect, test } from "bun:test";
import {
  parseMeteredImportStarted,
  parseMeteredMonth,
} from "./ai-metered-usage.ts";

const run = {
  runId: 7,
  kind: "admin",
  months: ["2026-09", "2026-10"],
  startedAt: "2026-10-25T10:00:00.123456Z",
  finishedAt: "2026-10-25T10:01:00Z",
  status: "succeeded",
  requests: 40,
  slicesFetched: 60,
  slicesChanged: 0,
  accountErrors: 0,
  error: null,
};

const group = {
  sku: "Copilot AI Credits",
  model: "gpt-6.1-sol",
  labels: ["Auto: GPT-6.1 Sol", "GPT-6.1 Sol"],
  kind: "trackable",
  status: "confirmed",
  gross: "1500.00",
  autoGross: "500.00",
  tracked: "1000.00",
  attributed: "1000.00",
  untracked: "500.00",
  ratio: 1.5,
  flags: [],
};

const account = {
  accountId: 1001,
  login: "ada-lovelace",
  owners: [
    {
      target: { kind: "user", userId: 3 },
      from: "2026-10-01",
      through: "2026-10-15",
    },
    { target: { kind: "external" }, from: "2026-10-16", through: "2026-10-31" },
  ],
  unlinkedDays: 0,
  daysNotImported: 1,
  gross: "1500.00",
  repositoryBilled: "0.00",
  untrackable: "0.00",
  tracked: "1000.00",
  trackedOldBasis: "250.00",
  trackedBeforeFloor: "0.00",
  unpricedRecords: 2,
  attributed: "1000.00",
  untracked: "500.00",
  unlinked: "0.00",
  external: "0.00",
  trackedShare: 0.6666666666666666,
  flags: ["stalePricing"],
  payerCheck: {
    checkedAt: "2026-10-25T10:00:30Z",
    payerGrossQuantity: "150000.000000001",
    storedGrossQuantity: "150000.000000001",
    matches: true,
    error: null,
  },
  groups: [
    group,
    {
      ...group,
      sku: null,
      model: "gpt-4o-mini-2024-07-18",
      labels: [],
      kind: "trackedOnly",
      status: "free",
      gross: "0.00",
      autoGross: "0.00",
      tracked: "0.50",
      attributed: "0.00",
      untracked: "0.00",
      ratio: 0,
    },
  ],
};

/** A month as the backend serializes it. */
const month = {
  provider: "copilot",
  namespace: "example-org",
  month: "2026-10",
  today: "2026-10-25",
  months: ["2026-09", "2026-10"],
  import: {
    status: "ready",
    message: null,
    backfillFrom: "2026-09-01",
    unattended: true,
    freshness: "stale",
    staleReason: {
      kind: "sourceBehind",
      source: "accountCredits",
      through: "2026-10-20",
    },
    running: null,
    lastRun: run,
    accountErrors: [
      {
        login: "grace-hopper",
        month: "2026-09",
        error: "not found",
        seated: false,
        interrupted: false,
      },
      {
        login: "grace-hopper",
        month: "2026-10",
        error: "not found",
        seated: true,
        interrupted: false,
      },
    ],
    sources: [
      {
        source: "accountCredits",
        importedThrough: "2026-10-20",
        lastSuccessAt: "2026-10-25T10:01:00Z",
        lastError: null,
        settleLagDays: 2,
      },
      {
        source: "summary",
        importedThrough: null,
        lastSuccessAt: null,
        lastError: "the usage source is unavailable: 502",
        settleLagDays: null,
      },
    ],
  },
  pool: {
    fetchedAt: "2026-10-25T10:00:10Z",
    exhaustedOn: "2026-10-12",
    credits: {
      grossCredits: "100000.00",
      discountCredits: "60000.00",
      netCredits: "40000.00",
      grossUsd: "1000.00",
      discountUsd: "600.00",
      netUsd: "400.00",
    },
    seats: { userMonths: "30", grossUsd: "570.00", netUsd: "570.00" },
    payerNetUsd: "970.00",
    unrecognized: [{ product: "copilot", sku: "new_sku", unit: "units" }],
  },
  totals: {
    gross: "1500.00",
    repositoryBilled: "0.00",
    tracked: "1000.50",
    trackedOldBasis: "250.00",
    trackedBeforeFloor: "0.00",
    attributed: "1000.00",
    untracked: "500.00",
    unlinked: "0.00",
    external: "0.00",
    unknownAccount: "-0.05",
    unknownAccountSettled: "0.00",
    incompleteImport: "5.00",
  },
  accounts: [account],
  unlinkedTracked: [{ userId: 4, tracked: "12.00", days: 3 }],
  repositories: [
    {
      repository: "widget-api",
      product: "copilot",
      sku: "Copilot AI Credits",
      unit: "AICredits",
      kind: "repositoryCredits",
      quantity: "1200",
      credits: "1200.00",
      grossUsd: "12.00",
      discountUsd: "12.00",
      netUsd: "0.00",
      days: 4,
    },
    {
      repository: "gadget-web",
      product: "actions",
      sku: "Actions Linux",
      unit: "Minutes",
      kind: "compute",
      quantity: "300.5",
      credits: null,
      grossUsd: "1.80",
      discountUsd: "1.80",
      netUsd: "0.00",
      days: 2,
    },
  ],
  codeReview: {
    byAccounts: "1210.00",
    byRepositories: "1200.00",
    days: [
      {
        day: "2026-10-23",
        byAccounts: "600.00",
        byRepositories: "600.00",
        difference: "0.00",
        status: "settled",
      },
      {
        day: "2026-10-24",
        byAccounts: "610.00",
        byRepositories: "600.00",
        difference: "-10.00",
        status: "provisional",
      },
    ],
  },
  residualDays: [
    {
      day: "2026-10-24",
      orgGross: "100.00",
      accountsGross: "100.05",
      residual: "-0.05",
      status: "provisional",
      accountsExceedOrg: false,
      groups: [
        { sku: "Copilot AI Credits", model: "gpt-6.1-sol", residual: "-0.05" },
      ],
    },
  ],
  orgCheck: {
    checkedAt: "2026-10-25T10:00:10Z",
    payerGrossQuantity: "151000.5",
    storedGrossQuantity: "151000",
    matches: false,
    error: null,
  },
};

describe("metered usage contract", () => {
  test("accepts the month the backend serializes", () => {
    expect(parseMeteredMonth(month)).toEqual(month);
  });

  test("accepts a running import and a month without imports", () => {
    const empty = {
      ...month,
      months: [],
      import: {
        ...month.import,
        freshness: "never",
        staleReason: null,
        running: { ...run, status: "running", finishedAt: null },
        lastRun: null,
        accountErrors: [],
        sources: [],
      },
      pool: null,
      accounts: [],
      unlinkedTracked: [],
      repositories: [],
      codeReview: null,
      residualDays: [],
      orgCheck: null,
    };
    expect(parseMeteredMonth(empty)).toEqual(empty);
  });

  test("takes a check that failed or compared, never half of each", () => {
    const failed = {
      ...month,
      orgCheck: {
        checkedAt: "2026-10-25T10:00:10Z",
        payerGrossQuantity: null,
        storedGrossQuantity: "151000",
        matches: null,
        error: "the usage source is unavailable: GitHub answered 502",
      },
    };
    expect(parseMeteredMonth(failed)).toEqual(failed);
    for (const orgCheck of [
      { ...failed.orgCheck, matches: true },
      { ...failed.orgCheck, payerGrossQuantity: "151000" },
      { ...month.orgCheck, error: "GitHub answered 502" },
      { ...month.orgCheck, matches: null },
      { ...failed.orgCheck, error: "" },
    ]) {
      expect(() => parseMeteredMonth({ ...month, orgCheck })).toThrow();
    }
    expect(() =>
      parseMeteredMonth({
        ...month,
        accounts: [
          { ...account, payerCheck: { ...account.payerCheck, matches: null } },
        ],
      }),
    ).toThrow();
  });

  test("refuses unknown keys, at the top and inside", () => {
    expect(() => parseMeteredMonth({ ...month, extra: 1 })).toThrow();
    expect(() =>
      parseMeteredMonth({ ...month, accounts: [{ ...account, extra: 1 }] }),
    ).toThrow();
    expect(() =>
      parseMeteredMonth({
        ...month,
        accounts: [{ ...account, groups: [{ ...group, extra: 1 }] }],
      }),
    ).toThrow();
  });

  test("takes code review per day, signed only in its difference", () => {
    const [settled] = month.codeReview.days;
    for (const day of [
      { ...settled, byAccounts: "-600.00" },
      { ...settled, difference: "0" },
      { ...settled, status: "late" },
      { ...settled, extra: 1 },
      { ...settled, day: "Oct 23" },
    ]) {
      expect(() =>
        parseMeteredMonth({
          ...month,
          codeReview: { ...month.codeReview, days: [day] },
        }),
      ).toThrow();
    }
    expect(() =>
      parseMeteredMonth({
        ...month,
        codeReview: { byAccounts: "0.00", byRepositories: "0.00" },
      }),
    ).toThrow();
  });

  test("allows a sign only on residuals and the unknown-account totals", () => {
    expect(() =>
      parseMeteredMonth({
        ...month,
        accounts: [{ ...account, gross: "-1.00" }],
      }),
    ).toThrow();
    expect(() =>
      parseMeteredMonth({
        ...month,
        totals: { ...month.totals, untracked: "-1.00" },
      }),
    ).toThrow();
    const negative = {
      ...month,
      totals: { ...month.totals, unknownAccountSettled: "-12.34" },
    };
    expect(parseMeteredMonth(negative)).toEqual(negative);
  });

  test("refuses credits that aren't hundredths and quantities that aren't exact", () => {
    for (const gross of ["1500", "1500.5", "1500.000", "1,500.00"]) {
      expect(() =>
        parseMeteredMonth({ ...month, accounts: [{ ...account, gross }] }),
      ).toThrow();
    }
    expect(() =>
      parseMeteredMonth({
        ...month,
        accounts: [
          {
            ...account,
            payerCheck: {
              ...account.payerCheck,
              payerGrossQuantity: "1e-05",
            },
          },
        ],
      }),
    ).toThrow();
  });

  test("takes every stale reason as structured data, never as text", () => {
    for (const staleReason of [
      { kind: "lastRunFailed", error: "GitHub answered 502." },
      { kind: "neverSucceeded" },
      { kind: "lastSuccessOld", at: "2026-10-25T07:00:00Z" },
      { kind: "sourceNotImported", source: "orgUsage" },
      { kind: "summaryNeverFetched" },
      { kind: "summaryOld", at: "2026-10-25T07:00:00Z" },
      {
        kind: "accountFailed",
        login: "ada-lovelace",
        month: "2026-10",
        error: "not found",
      },
    ]) {
      const stale = { ...month, import: { ...month.import, staleReason } };
      expect(parseMeteredMonth(stale)).toEqual(stale);
    }
    for (const staleReason of [
      "account credits imported through 2026-10-20",
      { kind: "lastSuccessOld", at: "yesterday" },
      { kind: "sourceBehind", source: "accountCredits" },
    ]) {
      expect(() =>
        parseMeteredMonth({
          ...month,
          import: { ...month.import, staleReason },
        }),
      ).toThrow();
    }
  });

  test("names the month of each account error", () => {
    const withoutMonth = { ...month.import.accountErrors[0] };
    delete withoutMonth.month;
    expect(() =>
      parseMeteredMonth({
        ...month,
        import: { ...month.import, accountErrors: [withoutMonth] },
      }),
    ).toThrow();
  });

  test("refuses runs in the wrong slot and owners that end before they start", () => {
    expect(() =>
      parseMeteredMonth({
        ...month,
        import: { ...month.import, running: run },
      }),
    ).toThrow();
    expect(() =>
      parseMeteredMonth({
        ...month,
        import: {
          ...month.import,
          lastRun: { ...run, status: "running", finishedAt: null },
        },
      }),
    ).toThrow();
    expect(() =>
      parseMeteredMonth({
        ...month,
        accounts: [
          {
            ...account,
            owners: [
              {
                target: { kind: "external" },
                from: "2026-10-16",
                through: "2026-10-15",
              },
            ],
          },
        ],
      }),
    ).toThrow();
  });

  test("parses a started import", () => {
    const started = {
      runId: 8,
      startedAt: "2026-10-25T10:05:00Z",
      months: ["2026-10"],
    };
    expect(parseMeteredImportStarted(started)).toEqual(started);
    expect(() =>
      parseMeteredImportStarted({ ...started, months: [] }),
    ).toThrow();
  });
});
