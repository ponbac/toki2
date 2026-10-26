import { describe, expect, test } from "bun:test";
import { formatInstant } from "./format.ts";
import {
  FLAG_LABELS,
  accountsWithErrors,
  blocksImport,
  canImport,
  checkState,
  codeReviewCheckTone,
  codeReviewDifferences,
  codeReviewDifferingDays,
  creditHundredths,
  creditsAsUsd,
  flagTone,
  formatCreditNumber,
  formatCredits,
  formatQuantity,
  formatRatio,
  formatShare,
  formatSignedCreditNumber,
  formatUsdAmount,
  freshnessBadge,
  freshnessText,
  hasUnrecognizedRepositoryRows,
  importedThrough,
  needsAttention,
  noDataReason,
  ownerDays,
  ownerLabel,
  residualDaysToShow,
  residualStatusLabel,
  staleReasonText,
} from "./reconciliation.ts";

function source(source, importedThrough) {
  return {
    source,
    importedThrough,
    lastSuccessAt: "2026-10-25T10:00:00Z",
    lastError: null,
    settleLagDays: null,
  };
}

const imported = {
  status: "ready",
  message: null,
  backfillFrom: "2026-09-01",
  unattended: true,
  freshness: "fresh",
  staleReason: null,
  running: null,
  lastRun: null,
  accountErrors: [],
  sources: [
    source("accountCredits", "2026-10-24"),
    source("orgCredits", "2026-10-23"),
    source("orgUsage", "2026-10-24"),
    source("summary", null),
  ],
};

const zeroTotals = {
  gross: "0.00",
  repositoryBilled: "0.00",
  tracked: "0.00",
  trackedOldBasis: "0.00",
  trackedBeforeFloor: "0.00",
  attributed: "0.00",
  untracked: "0.00",
  unlinked: "0.00",
  external: "0.00",
  unknownAccount: "0.00",
  unknownAccountSettled: "0.00",
  incompleteImport: "0.00",
};

const equalCheck = {
  checkedAt: "2026-10-25T10:00:00Z",
  payerGrossQuantity: "100",
  storedGrossQuantity: "100",
  matches: true,
  error: null,
};

function month({
  flags = [],
  unknownAccount = "0.00",
  orgCheck = equalCheck,
  ...importChanges
} = {}) {
  return {
    import: { ...imported, ...importChanges },
    totals: { ...zeroTotals, unknownAccount },
    accounts: [{ flags }],
    orgCheck,
  };
}

describe("credits", () => {
  test("parse as signed hundredths", () => {
    expect(creditHundredths("1234.56")).toBe(123456);
    expect(creditHundredths("-0.05")).toBe(-5);
    expect(creditHundredths("0.00")).toBe(0);
  });

  test("format with grouping, a sign and their unit", () => {
    expect(formatCreditNumber("1234567.89")).toBe("1,234,567.89");
    expect(formatCreditNumber("-1234.05")).toBe("-1,234.05");
    expect(formatCredits("1234.56")).toBe("1,234.56 credits");
    expect(formatCredits("1.00")).toBe("1.00 credit");
    expect(formatCredits("-0.05")).toBe("-0.05 credits");
    expect(formatSignedCreditNumber("1234.50")).toBe("+1,234.50");
    expect(formatSignedCreditNumber("-10.00")).toBe("-10.00");
    expect(formatSignedCreditNumber("0.00")).toBe("0.00");
  });

  test("are worth $0.01 each, rounded to the cent half away from zero", () => {
    expect(creditsAsUsd("1500.00")).toBe("$15.00");
    expect(creditsAsUsd("1234.50")).toBe("$12.35");
    expect(creditsAsUsd("1234.49")).toBe("$12.34");
    expect(creditsAsUsd("-1234.50")).toBe("-$12.35");
    expect(creditsAsUsd("0.49")).toBe("$0.00");
  });

  test("exact USD amounts show as dollars", () => {
    expect(formatUsdAmount("1234.50")).toBe("$1,234.50");
    expect(formatUsdAmount("0.00")).toBe("$0.00");
  });

  test("GitHub's exact quantities are grouped and rounded for reading", () => {
    expect(formatQuantity("33.451612903")).toBe("33.45");
    expect(formatQuantity("123456.789012345")).toBe("123,456.79");
    expect(formatQuantity("30")).toBe("30");
    expect(formatQuantity("0.000000001")).toBe("0");
  });

  test("shares and ratios show a dash without a value", () => {
    expect(formatShare(null)).toBe("—");
    expect(formatShare(0.5)).toBe("50%");
    expect(formatShare(0.004)).toBe("under 1%");
    expect(formatRatio(null)).toBe("—");
    expect(formatRatio(0.9)).toBe("0.90");
  });
});

describe("labels", () => {
  test("name every flag and give import problems the loudest tone", () => {
    expect(FLAG_LABELS.trackedNotBilled).toBe("Tracked, GitHub billed nothing");
    expect(FLAG_LABELS.lowRatio).toBe("GitHub below 85% of tracked");
    expect(flagTone("importFailed")).toBe("critical");
    expect(flagTone("external")).toBe("neutral");
    expect(flagTone("stalePricing")).toBe("warning");
  });

  test("name residual statuses and owners", () => {
    expect(residualStatusLabel("provisional")).toBe("Still settling");
    expect(residualStatusLabel("incomplete")).toBe("Import gap");
    const names = new Map([[3, "Ada Lovelace"]]);
    const ada = {
      target: { kind: "user", userId: 3 },
      from: "2026-10-01",
      through: "2026-10-31",
    };
    expect(ownerLabel(ada, names)).toBe("Ada Lovelace");
    expect(
      ownerLabel({ ...ada, target: { kind: "user", userId: 4 } }, names),
    ).toBe("User 4");
    expect(ownerLabel({ ...ada, target: { kind: "external" } }, names)).toBe(
      "External",
    );
    expect(ownerDays(ada, "2026-10", "2026-11-02")).toBeNull();
    expect(
      ownerDays({ ...ada, through: "2026-10-15" }, "2026-10", "2026-11-02"),
    ).toBe("Oct 1 – Oct 15");
    // The current month is whole through today.
    expect(
      ownerDays({ ...ada, through: "2026-10-25" }, "2026-10", "2026-10-25"),
    ).toBeNull();
    expect(
      ownerDays(
        { ...ada, from: "2026-10-20", through: "2026-10-25" },
        "2026-10",
        "2026-10-25",
      ),
    ).toBe("Oct 20 – Oct 25");
  });
});

describe("freshness", () => {
  test("is imported through the earliest daily source", () => {
    expect(importedThrough(imported)).toBe("2026-10-23");
    expect(freshnessText(imported)).toBe("Imported through Oct 23 (UTC)");
    expect(
      importedThrough({
        ...imported,
        sources: [source("accountCredits", "2026-10-24")],
      }),
    ).toBeNull();
  });

  test("says why there is nothing to show", () => {
    expect(
      freshnessText({ ...imported, status: "disabled", freshness: "off" }),
    ).toContain("isn't connected");
    expect(
      freshnessText({
        ...imported,
        status: "misconfigured",
        message: "bad key",
        freshness: "off",
      }),
    ).toBe("GitHub is misconfigured: bad key");
    expect(freshnessText({ ...imported, freshness: "never" })).toBe(
      "Nothing imported from GitHub yet",
    );
  });

  test("badges each state", () => {
    expect(freshnessBadge(imported)).toEqual({ tone: "good", label: "Fresh" });
    expect(freshnessBadge({ ...imported, freshness: "stale" }).tone).toBe(
      "warning",
    );
  });

  test("phrases each stale reason as a capitalized clause", () => {
    const at = "2026-10-25T07:00:00Z";
    const texts = [
      staleReasonText({
        kind: "lastRunFailed",
        error: "GitHub answered 502: try again later. ",
      }),
      staleReasonText({ kind: "neverSucceeded" }),
      staleReasonText({ kind: "lastSuccessOld", at }),
      staleReasonText({ kind: "sourceNotImported", source: "orgUsage" }),
      staleReasonText({
        kind: "sourceBehind",
        source: "accountCredits",
        through: "2026-10-20",
      }),
      staleReasonText({ kind: "summaryNeverFetched" }),
      staleReasonText({ kind: "summaryOld", at }),
      staleReasonText({
        kind: "accountFailed",
        login: "ada-lovelace",
        month: "2026-10",
        error: "not found.",
      }),
    ];
    expect(texts[0]).toBe(
      "The last import failed: GitHub answered 502: try again later",
    );
    expect(texts[2]).toBe(`The last import succeeded at ${formatInstant(at)}`);
    expect(texts[3]).toBe(
      "No day of organization usage is completely imported yet",
    );
    expect(texts[4]).toBe(
      "Account credits: imported only through Oct 20 (UTC)",
    );
    expect(texts[7]).toBe(
      "ada-lovelace couldn't be imported for October 2026: not found",
    );
    for (const text of texts) {
      // A login may start the sentence; any other reason is capitalized.
      expect(text.startsWith("ada-lovelace") || /^[A-Z]/.test(text)).toBe(true);
      expect(text).not.toMatch(/\d{2}:\d{2}:\d{2}Z|\.$/);
    }
  });

  test("counts accounts, not account-months, among import errors", () => {
    const error = (login, month) => ({
      login,
      month,
      error: "not found",
      seated: true,
      interrupted: false,
    });
    expect(
      accountsWithErrors([
        error("grace-hopper", "2026-09"),
        error("grace-hopper", "2026-10"),
        error("alan-turing", "2026-10"),
      ]),
    ).toBe(2);
  });

  test("an account's own error counts with a seat, a stopped run's always", () => {
    const error = (seated, interrupted) => ({
      login: "grace-hopper",
      month: "2026-09",
      error: "not found",
      seated,
      interrupted,
    });
    expect(blocksImport(error(true, false))).toBe(true);
    expect(blocksImport(error(false, false))).toBe(false);
    expect(blocksImport(error(false, true))).toBe(true);
  });

  test("says why a month has no GitHub data, never that an account is missing", () => {
    const data = (months, status = "ready") => ({
      months,
      today: "2026-10-25",
      import: { ...imported, status },
    });
    expect(noDataReason(data(["2026-09", "2026-10"]), "2026-10")).toBeNull();
    expect(noDataReason(data(["2026-10"]), "2026-06")).toBe("beforeBackfill");
    expect(noDataReason(data(["2026-10"]), "2026-11")).toBe("notStarted");
    expect(noDataReason(data(["2026-10"]), "2026-09")).toBe("notImported");
    expect(noDataReason(data([], "disabled"), "2026-10")).toBe("sourceOff");
  });

  test("imports only months from the backfill through today", () => {
    expect(canImport(imported, "2026-09", "2026-10-25")).toBe(true);
    expect(canImport(imported, "2026-08", "2026-10-25")).toBe(false);
    expect(canImport(imported, "2026-11", "2026-10-25")).toBe(false);
    expect(
      canImport({ ...imported, status: "disabled" }, "2026-10", "2026-10-25"),
    ).toBe(false);
  });
});

describe("attention", () => {
  test("a clean month needs none", () => {
    expect(needsAttention(month())).toBe(false);
  });

  test("a stale or never-run import, a flag or unknown credits need a look", () => {
    expect(
      needsAttention(
        month({
          freshness: "stale",
          staleReason: { kind: "neverSucceeded" },
        }),
      ),
    ).toBe(true);
    expect(needsAttention(month({ freshness: "never" }))).toBe(true);
    expect(needsAttention(month({ flags: ["lowRatio"] }))).toBe(true);
    expect(needsAttention(month({ unknownAccount: "-0.05" }))).toBe(true);
  });

  test("an org total that differs from GitHub's or failed its check needs a look", () => {
    const differs = {
      ...equalCheck,
      payerGrossQuantity: "101",
      matches: false,
    };
    const failed = {
      ...equalCheck,
      payerGrossQuantity: null,
      matches: null,
      error: "the usage source is unavailable",
    };
    expect(needsAttention(month({ orgCheck: differs }))).toBe(true);
    expect(needsAttention(month({ orgCheck: failed }))).toBe(true);
    expect(needsAttention(month({ orgCheck: null }))).toBe(false);
    expect(checkState(equalCheck)).toBe("equal");
    expect(checkState(differs)).toBe("differs");
    expect(checkState(failed)).toBe("failed");
    expect(checkState(null)).toBe("unchecked");
  });

  test("an external account or a disconnected source needs none", () => {
    expect(needsAttention(month({ flags: ["external"] }))).toBe(false);
    expect(
      needsAttention(month({ status: "disabled", freshness: "off" })),
    ).toBe(false);
  });

  test("residual rows are days with a residual or not yet settled", () => {
    const day = (day, residual, status) => ({
      day,
      orgGross: "1.00",
      accountsGross: "1.00",
      residual,
      status,
      accountsExceedOrg: false,
      groups: [],
    });
    const days = [
      day("2026-10-01", "0.00", "settled"),
      day("2026-10-02", "5.00", "settled"),
      day("2026-10-03", "0.00", "provisional"),
    ];
    expect(residualDaysToShow(days).map((d) => d.day)).toEqual([
      "2026-10-02",
      "2026-10-03",
    ]);
  });

  test("code review differs on days whose difference is not zero, settled or not", () => {
    const day = (day, difference, status) => ({
      day,
      byAccounts: "10.00",
      byRepositories: "10.00",
      difference,
      status,
    });
    const days = [
      day("2026-10-01", "0.00", "settled"),
      day("2026-10-02", "-10.00", "settled"),
      day("2026-10-03", "0.00", "provisional"),
      day("2026-10-04", "0.05", "incomplete"),
    ];
    expect(codeReviewDifferingDays(days).map((d) => d.day)).toEqual([
      "2026-10-02",
      "2026-10-04",
    ]);
  });

  test("code review differences are told apart by their sign", () => {
    const day = (difference) => ({
      day: "2026-10-01",
      byAccounts: "10.00",
      byRepositories: "10.00",
      difference,
      status: "settled",
    });
    const adds = [day("0.00")];
    // Repositories above accounts: unknown accounts' code review, billed
    // once through its repository line.
    const unknownAccounts = [day("0.00"), day("4.00")];
    // Accounts above repositories: code review billed on no line.
    const withoutRepository = [day("4.00"), day("-2.50")];

    expect(codeReviewDifferences(adds)).toEqual({
      accountsExceed: false,
      repositoriesExceed: false,
    });
    expect(codeReviewDifferences(unknownAccounts)).toEqual({
      accountsExceed: false,
      repositoriesExceed: true,
    });
    expect(codeReviewDifferences(withoutRepository)).toEqual({
      accountsExceed: true,
      repositoriesExceed: true,
    });
    expect(codeReviewCheckTone(adds)).toBe("good");
    expect(codeReviewCheckTone(unknownAccounts)).toBe("neutral");
    expect(codeReviewCheckTone(withoutRepository)).toBe("warning");
    expect(codeReviewCheckTone([day("-0.01")])).toBe("warning");
  });
});

describe("hasUnrecognizedRepositoryRows", () => {
  const row = (kind, repository) => ({ kind, repository });

  test("flags an unrecognized row naming a repository", () => {
    expect(
      hasUnrecognizedRepositoryRows([
        row("repositoryCredits", "widget-api"),
        row("unrecognized", "gadget-web"),
      ]),
    ).toBe(true);
  });

  test("passes recognized rows and unrecognized ones without a repository", () => {
    expect(
      hasUnrecognizedRepositoryRows([
        row("repositoryCredits", "widget-api"),
        row("compute", "widget-api"),
        row("unrecognized", ""),
      ]),
    ).toBe(false);
  });
});
