import { describe, expect, test } from "bun:test";
import {
  formatLineCredits,
  groupLines,
  lineProject,
  lineUserId,
  lineWarnings,
  meteredLineLabel,
  subjectKey,
  subjectLabel,
  trackedSkus,
} from "./billing.ts";

const usage = {
  apiEquivalentUsd: 1,
  unpricedRecords: 0,
  records: 1,
  tokens: { input: 1, cacheRead: 0, cacheWrite: 0, output: 1, total: 2 },
};

const ADA = { kind: "user", userId: 1 };
const CONTRACTOR = {
  kind: "providerAccount",
  provider: "copilot",
  accountId: 2002,
  login: "kj-contractor",
};
const UNKNOWN = { kind: "unknownAccount", provider: "copilot" };
const WIDGET_API = {
  kind: "repository",
  provider: "copilot",
  name: "widget-api",
};

/** A metered line of `kind` for `subject`, settled unless `metered` says. */
function line(kind, subject, extra = {}) {
  const { metered = {}, ...rest } = extra;
  return {
    subject,
    provider: "copilot",
    project: null,
    unassigned: null,
    billingMode: "metered",
    unallocatedOverhead: false,
    subscriptionId: null,
    metered: {
      kind,
      credits: "1235.00",
      account:
        subject.kind === "unknownAccount" || subject.kind === "repository"
          ? null
          : { accountId: 1001, login: "ada-lovelace" },
      sku: "Copilot AI Credits",
      provisional: false,
      provisionalReasons: [],
      lowRatio: false,
      ...metered,
    },
    billableAmount: "12.35",
    billableCurrency: "USD",
    convertedAmount: "129.68",
    exchangeRate: "10.50",
    missingRate: false,
    usage,
    ...rest,
  };
}

const project = {
  projectId: "101",
  projectName: "Example Client - Widget API",
};

describe("lineProject for a metered payer's lines", () => {
  test.each([
    [
      "a project's credits bill the project",
      line("project", ADA, { project }),
      ["project:101", "project", null, null],
    ],
    [
      "credits outside any repository stay Unassigned",
      line("project", ADA, { unassigned: "unattributed" }),
      ["unassigned:unattributed", "unassigned", "unattributed", null],
    ],
    [
      "untracked credits are Spinit's",
      line("untracked", ADA),
      ["spinit:untracked", "spinit", null, "untracked"],
    ],
    [
      "personal credits are Spinit's",
      line("personal", ADA),
      ["spinit:personal", "spinit", null, "personal"],
    ],
    [
      "an unlinked account's credits are Spinit's",
      line("unlinkedAccount", CONTRACTOR),
      ["spinit:accounts", "spinit", null, "accounts"],
    ],
    [
      "an external account's credits are Spinit's",
      line("externalAccount", CONTRACTOR),
      ["spinit:accounts", "spinit", null, "accounts"],
    ],
    [
      "the unknown account's credits are Spinit's",
      line("unknownAccount", UNKNOWN),
      ["spinit:accounts", "spinit", null, "accounts"],
    ],
    [
      "a mapped repository's code review bills its project",
      line("repository", WIDGET_API, { project }),
      ["project:101", "project", null, null],
    ],
    [
      "an unmapped repository's code review stays Unassigned on its own",
      line("repository", WIDGET_API, { unassigned: "unmappedRepository" }),
      [
        "unassigned:unmappedRepository",
        "unassigned",
        "unmappedRepository",
        null,
      ],
    ],
  ])("%s", (_, metered, expected) => {
    const { key, kind, unassigned, spinit } = lineProject(metered);
    expect([key, kind, unassigned, spinit]).toEqual(expected);
  });
});

describe("subjects", () => {
  const names = new Map([[1, "Ada Lovelace"]]);

  test("keys are unique across kinds and labels name each", () => {
    const subjects = [ADA, CONTRACTOR, UNKNOWN, WIDGET_API];

    expect(subjects.map(subjectKey)).toEqual([
      "user:1",
      "account:copilot:2002",
      "unknown:copilot",
      "repository:copilot:widget-api",
    ]);
    expect(subjects.map((subject) => subjectLabel(subject, names))).toEqual([
      "Ada Lovelace",
      "kj-contractor",
      "Unknown accounts",
      "Copilot code review · widget-api",
    ]);
    expect(subjectLabel({ kind: "user", userId: 9 }, names)).toBe("User 9");
  });

  test("only a developer's line has a user", () => {
    expect(lineUserId(line("untracked", ADA))).toBe(1);
    expect(lineUserId(line("externalAccount", CONTRACTOR))).toBeNull();
  });

  test("grouped by subject, developers come before accounts and repositories", () => {
    const groups = groupLines(
      [
        line("repository", WIDGET_API, { project }),
        line("externalAccount", CONTRACTOR),
        line("project", ADA, { project }),
        line("untracked", ADA),
      ],
      [{ userId: 1, fullName: "Ada Lovelace", email: "ada@example.com" }],
      "developer",
    );

    expect(groups.map((g) => [g.key, g.kind, g.label, g.userId])).toEqual([
      ["subject:user:1", "developer", "Ada Lovelace", 1],
      [
        "subject:repository:copilot:widget-api",
        "account",
        "Copilot code review · widget-api",
        undefined,
      ],
      ["subject:account:copilot:2002", "account", "kj-contractor", undefined],
    ]);
  });
});

describe("metered wording", () => {
  test("credits are whole as billed", () => {
    expect(formatLineCredits("1235.00")).toBe("1,235 credits");
    expect(formatLineCredits("1.00")).toBe("1 credit");
    expect(formatLineCredits("0.50")).toBe("0.50 credits");
  });

  test("a line names its SKU only when Toki does not track it", () => {
    const lines = [
      line("project", ADA, { project }),
      line("untracked", ADA),
      line("untracked", ADA, { metered: { sku: "Copilot Cloud Agent" } }),
      line("unknownAccount", UNKNOWN, { metered: { sku: null } }),
    ];
    const tracked = trackedSkus(lines);

    expect([...tracked]).toEqual(["Copilot AI Credits"]);
    expect(lines.map((l) => meteredLineLabel(l, tracked))).toEqual([
      "GitHub Copilot · 1,235 credits",
      "GitHub Copilot · 1,235 credits",
      "GitHub Copilot · 1,235 credits · Copilot Cloud Agent",
      "GitHub Copilot · 1,235 credits",
    ]);
    // Without a project's line to tell, every SKU is named.
    expect(meteredLineLabel(lines[1], trackedSkus([lines[1]]))).toBe(
      "GitHub Copilot · 1,235 credits · Copilot AI Credits",
    );
  });

  test("warnings say why a line may change and what to look into", () => {
    expect(
      lineWarnings(
        line("project", ADA, {
          project,
          usage: { ...usage, unpricedRecords: 4 },
          metered: {
            provisional: true,
            provisionalReasons: ["importIncomplete", "oldPricingBasis"],
            lowRatio: true,
          },
        }),
        undefined,
      ),
    ).toEqual([
      "Provisional: GitHub's usage is not imported for every day; some tracked usage is on an old pricing basis.",
      "GitHub billed under 85% of what Toki tracked on a settled day.",
      "4 unpriced records carry no weight in the split.",
    ]);
    expect(lineWarnings(line("unknownAccount", UNKNOWN), undefined)).toEqual([
      "Code review is billed by repository and left out.",
    ]);
    expect(lineWarnings(line("project", ADA, { project }), undefined)).toEqual(
      [],
    );
    expect(
      lineWarnings(
        line("repository", WIDGET_API, { unassigned: "unmappedRepository" }),
        undefined,
      ),
    ).toEqual(["No project: map this repository under Project mappings."]);
    expect(
      lineWarnings(line("repository", WIDGET_API, { project }), undefined),
    ).toEqual([]);
  });
});
