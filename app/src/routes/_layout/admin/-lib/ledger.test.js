import { describe, expect, test } from "bun:test";
import {
  SPINIT_OTHER_KEY,
  SPINIT_UNATTRIBUTED_KEY,
  allocationSegments,
  ledgerEntries,
  ledgerOf,
  peopleOf,
  segmentKeys,
  segmentSteps,
  spinitShare,
  splitProjectName,
  statementUnassigned,
  subjectsLabel,
} from "./ledger.ts";

const usage = {
  apiEquivalentUsd: 1,
  unpricedRecords: 0,
  records: 1,
  tokens: { input: 1, cacheRead: 0, cacheWrite: 0, output: 1, total: 2 },
};

/** A developer's line; `userId` in `extra` names the developer. */
function line(projectName, sek, extra = {}) {
  const { userId = 1, ...rest } = extra;
  return {
    subject: { kind: "user", userId },
    provider: "codex",
    project: projectName ? { projectId: projectName, projectName } : null,
    unassigned: projectName ? null : "unmapped",
    billingMode: "api",
    unallocatedOverhead: false,
    subscriptionId: null,
    metered: null,
    billableAmount: sek,
    billableCurrency: "SEK",
    convertedAmount: sek,
    exchangeRate: null,
    missingRate: false,
    usage,
    ...rest,
  };
}

describe("ledgerOf", () => {
  test("ranks projects dearest first and keeps Unassigned and overhead apart", () => {
    const ledger = ledgerOf([
      line("Small - A", "10.00"),
      line(null, "500.00"),
      line("Big - B", "300.00"),
      line("Big - B", "200.00", { userId: 2 }),
      line("Unpriced - C", null, { convertedAmount: null }),
      line(null, "50.00", {
        unallocatedOverhead: true,
        unassigned: null,
        billingMode: "subscription",
      }),
    ]);

    expect(ledger.projects.map((e) => e.label)).toEqual([
      "Big - B",
      "Small - A",
      "Unpriced - C",
    ]);
    expect(ledger.projects[0].totals.converted).toBe(50000);
    expect(ledger.projects[0].people.map((p) => p.userId)).toEqual([1, 2]);
    expect(ledger.unassigned.map((e) => e.totals.converted)).toEqual([50000]);
    expect(ledger.overhead?.totals.converted).toBe(5000);
  });

  test("keeps usage outside any repository apart from unmapped repositories", () => {
    const ledger = ledgerOf([
      line(null, "300.00", { unassigned: "unattributed" }),
      line(null, "20.00"),
      line(null, "5.00", { unassigned: "unattributed", userId: 2 }),
      line("Big - B", "100.00"),
    ]);

    expect(
      ledger.unassigned.map((e) => [
        e.key,
        e.unassigned,
        e.label,
        e.totals.converted,
        e.people.map((p) => p.userId),
      ]),
    ).toEqual([
      ["unassigned:unmapped", "unmapped", "Unmapped repositories", 2000, [1]],
      [
        "unassigned:unattributed",
        "unattributed",
        "No repository",
        30500,
        [1, 2],
      ],
    ]);
    expect(ledger.totals.converted).toBe(42500);
  });

  test("groups an entry's lines by developer, dearest first", () => {
    const [entry] = ledgerOf([
      line("P - X", "30.00", { userId: 1, provider: "pi" }),
      line("P - X", "500.00", { userId: 2 }),
      line("P - X", "400.00", { userId: 1, provider: "claude" }),
      line("P - X", "200.00", { userId: 1 }),
      line("P - X", null, {
        userId: 3,
        missingRate: true,
        billableCurrency: "USD",
      }),
      line("P - X", "80.00", { userId: 1, billingMode: "subscription" }),
    ]).projects;

    expect(
      entry.people.map((p) => [
        p.userId,
        p.totals.converted,
        p.lines.map((l) => l.convertedAmount),
        p.providers,
      ]),
    ).toEqual([
      [
        1,
        71000,
        ["400.00", "200.00", "80.00", "30.00"],
        ["codex", "claude", "pi"],
      ],
      [2, 50000, ["500.00"], ["codex"]],
      [3, null, [null], ["codex"]],
    ]);
  });
});

/** A month's ledger from `[name, sek]` pairs; a null name is Unassigned,
 * of unmapped repositories unless a third element names its kind. */
function month(rows) {
  return ledgerOf(
    rows.map(([name, sek, unassigned]) =>
      line(name, sek, unassigned ? { unassigned } : {}),
    ),
  );
}

const steps = (segments) =>
  segments.map((s) => [s.kind, s.label, s.step, s.entries.length]);

const OCTOBER = [
  ["Norrsken Logistik - Kundportal", "6454.93"],
  ["Tallbo Fastighet - Underhåll", "1901.18"],
  ["Ljusdal Energi - Driftstöd", "1532.07"],
  ["Bo-Ek Bygg - Offertverktyg", "1259.38"],
  ["Glimmer Retail - Butiksapp", "629.85"],
  ["Stensjö Vård - Bokning", "246.17"],
  ["Hamnverket - Integrationer", "210.75"],
  ["Kvarnens Mat - E-handel", "135.42"],
  ["Ekdal Media - Webbplats", "8.72"],
  [null, "6880.84"],
];

const SEPTEMBER = [
  ["Norrsken Logistik - Kundportal", "8755.80"],
  ["Ljusdal Energi - Driftstöd", "1021.80"],
  ["Tallbo Fastighet - Underhåll", "784.30"],
  ["Glimmer Retail - Butiksapp", "372.49"],
  ["Stensjö Vård - Bokning", "328.65"],
  ["Kvarnens Mat - E-handel", "142.98"],
  ["Hamnverket - Integrationer", "59.31"],
  ["Nordhamn Drift", "8.39"],
  [null, "7588.09"],
];

describe("allocationSegments", () => {
  test("October: four projects of their own, the rest named after the dearest", () => {
    expect(steps(allocationSegments(month(OCTOBER)))).toEqual([
      ["project", "Norrsken Logistik - Kundportal", 1, 1],
      ["project", "Tallbo Fastighet - Underhåll", 2, 1],
      ["project", "Ljusdal Energi - Driftstöd", 3, 1],
      ["project", "Bo-Ek Bygg - Offertverktyg", 4, 1],
      // Glimmer Retail is 3% of the bill: merged by the cap, not by its size.
      ["others", "Glimmer Retail + 4 more", 5, 5],
      ["unassigned", "Unmapped repositories", null, 1],
    ]);
  });

  test("September: a tail merged by the cap is named after its dearest", () => {
    expect(steps(allocationSegments(month(SEPTEMBER)))).toEqual([
      ["project", "Norrsken Logistik - Kundportal", 1, 1],
      ["project", "Ljusdal Energi - Driftstöd", 2, 1],
      ["project", "Tallbo Fastighet - Underhåll", 3, 1],
      ["project", "Glimmer Retail - Butiksapp", 4, 1],
      ["others", "Stensjö Vård + 3 more", 5, 4],
      ["unassigned", "Unmapped repositories", null, 1],
    ]);
  });

  test("a tail of only small projects says so, and keeps step 5 after fewer than four", () => {
    const segments = allocationSegments(
      month([
        ["Big - B", "6000.00"],
        ["Mid - M", "3700.00"],
        ["Tiny - T", "100.00"],
        ["Wee - W", "100.00"],
        ["Mini - I", "100.00"],
        ["Free - F", "0.00"],
      ]),
    );

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["project", "Mid - M", 2, 1],
      ["others", "3 smaller projects", 5, 3],
    ]);
    expect(segments[2].amount).toBe(30000);
  });

  test("a single project left over keeps its own segment on the tail's step", () => {
    const segments = allocationSegments(
      month([
        ["Big - B", "9900.00"],
        ["Tiny - T", "10.00"],
      ]),
    );

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["project", "Tiny - T", 5, 1],
    ]);
    expect(segments[1].key).toBe("project:Tiny - T");
  });

  test("thirty projects still give four of their own and one tail, stepped without gaps", () => {
    const rows = Array.from({ length: 30 }, (_, index) => [
      `Customer ${index} - Project`,
      index < 6 ? `${1000 - index * 10}.00` : "10.00",
    ]);
    const segments = allocationSegments(month(rows));
    const stepped = segments.map((s) => s.step);

    expect(stepped).toEqual([1, 2, 3, 4, 5]);
    expect(segments[4].entries).toHaveLength(26);
    expect(segments[4].label).toBe("Customer 4 + 25 more");
  });

  test("short names: a project by its customer, the tail and Unassigned in a word or two", () => {
    expect(
      allocationSegments(month(OCTOBER_BY_KIND)).map((s) => [s.short, s.more]),
    ).toEqual([
      ["Norrsken Logistik", 0],
      ["Tallbo Fastighet", 0],
      ["Ljusdal Energi", 0],
      ["Bo-Ek Bygg", 0],
      ["Glimmer Retail", 4],
      ["Unmapped", 0],
      ["No repo", 0],
    ]);
    const small = allocationSegments(
      month([
        ["Big - B", "9700.00"],
        ["Tiny - T", "100.00"],
        ["Wee - W", "100.00"],
        ["Mini - I", "100.00"],
      ]),
    );
    expect(small.map((s) => [s.short, s.more])).toEqual([
      ["Big", 0],
      ["3 smaller", 0],
    ]);
  });
});

describe("allocationSegments without the whole bill", () => {
  test("a line without an exchange rate leaves no strip, so no share reads as the whole bill", () => {
    const ledger = ledgerOf([
      line("Big - B", "900.00"),
      line("Foreign - F", null, { missingRate: true, convertedAmount: null }),
    ]);
    const segments = allocationSegments(ledger);

    expect(ledger.totals.converted).toBeNull();
    expect(segments).toEqual([]);
    expect(segmentSteps(segments).size).toBe(0);
  });
});

/** October with its Unassigned 6880.84 SEK in two kinds, most of it
 * outside any repository. */
const OCTOBER_BY_KIND = [
  ...OCTOBER.filter(([name]) => name !== null),
  [null, "880.84"],
  [null, "6000.00", "unattributed"],
];

describe("allocationSegments by Unassigned kind", () => {
  test("October: both kinds follow the projects without a step", () => {
    const segments = allocationSegments(month(OCTOBER_BY_KIND));

    expect(steps(segments)).toEqual([
      ["project", "Norrsken Logistik - Kundportal", 1, 1],
      ["project", "Tallbo Fastighet - Underhåll", 2, 1],
      ["project", "Ljusdal Energi - Driftstöd", 3, 1],
      ["project", "Bo-Ek Bygg - Offertverktyg", 4, 1],
      ["others", "Glimmer Retail + 4 more", 5, 5],
      ["unassigned", "Unmapped repositories", null, 1],
      ["unassigned", "No repository", null, 1],
    ]);
    expect(segments.slice(5).map((s) => s.amount)).toEqual([88084, 600000]);
    // The parts add up to October's Unassigned.
    expect(segments[5].amount + segments[6].amount).toBe(688084);
    expect(
      [...segmentSteps(segments).keys()].some((key) =>
        key.startsWith("unassigned"),
      ),
    ).toBe(false);
  });

  test("a no-repository line without an exchange rate leaves no strip either", () => {
    const ledger = ledgerOf([
      line("Big - B", "900.00"),
      line(null, "20.00"),
      line(null, null, {
        unassigned: "unattributed",
        missingRate: true,
        convertedAmount: null,
      }),
    ]);

    expect(ledger.unassigned).toHaveLength(2);
    expect(allocationSegments(ledger)).toEqual([]);
  });
});

describe("segmentSteps", () => {
  test("every row maps to its segment's step; Unassigned, overhead and unknown amounts do not", () => {
    const ledger = ledgerOf([
      ...OCTOBER.map(([name, sek]) => line(name, sek)),
      line("Unpriced - U", null, { convertedAmount: null }),
      line(null, "50.00", {
        unallocatedOverhead: true,
        billingMode: "subscription",
      }),
    ]);
    const segments = allocationSegments(ledger);
    const map = segmentSteps(segments);

    for (const segment of segments) {
      for (const entry of segment.entries) {
        expect(map.get(entry.key) ?? null).toBe(segment.step);
      }
    }
    expect(map.get("project:Glimmer Retail - Butiksapp")).toBe(5);
    expect(map.get("project:Ekdal Media - Webbplats")).toBe(5);
    expect(map.has("unassigned:unmapped")).toBe(false);
    expect(map.has("overhead")).toBe(false);
    expect(map.has("project:Unpriced - U")).toBe(false);
  });
});

const ADA = { accountId: 1001, login: "ada-lovelace" };

/**
 * A metered payer's line of `kind`, with its subject and Unassigned kind as
 * the server sends them: a developer's for a project, personal or untracked
 * credits, an account's for unlinked or external credits, and the unknown
 * account's for its residual.
 */
function metered(kind, projectName, sek, extra = {}) {
  const { userId = 1, account = ADA, ...rest } = extra;
  const accountKinds = ["unlinkedAccount", "externalAccount"];
  const subject = accountKinds.includes(kind)
    ? { kind: "providerAccount", provider: "copilot", ...account }
    : kind === "unknownAccount"
      ? { kind: "unknownAccount", provider: "copilot" }
      : { kind: "user", userId };
  const project =
    projectName === null ? null : { projectId: projectName, projectName };
  return line(null, sek, {
    subject,
    provider: "copilot",
    project,
    unassigned: kind === "project" && project === null ? "unattributed" : null,
    billingMode: "metered",
    billableCurrency: "USD",
    metered: {
      kind,
      credits: `${Math.round(Number(sek) * 10)}.00`,
      account: kind === "unknownAccount" ? null : account,
      sku: "Copilot AI Credits",
      provisional: false,
      provisionalReasons: [],
      lowRatio: false,
    },
    ...rest,
  });
}

/** A month with a metered payer's lines: a project's, Unassigned outside any
 * repository, and Spinit's of every kind. */
function meteredMonth() {
  return [
    line("Big - B", "300.00"),
    metered("project", "Big - B", "200.00"),
    metered("project", null, "40.00"),
    line(null, "10.00", { unassigned: "unattributed" }),
    metered("untracked", null, "120.00"),
    metered("untracked", null, "30.00", { userId: 2 }),
    metered("personal", null, "25.00"),
    metered("unlinkedAccount", null, "15.00", {
      account: { accountId: 2002, login: "grace-hopper" },
    }),
    metered("externalAccount", null, "5.00", {
      account: { accountId: 3003, login: "kj-contractor" },
    }),
    metered("unknownAccount", null, "2.00"),
  ];
}

describe("a metered payer's lines", () => {
  test("a project's credits join its entry; Spinit's follow every other entry by kind", () => {
    const ledger = ledgerOf(meteredMonth());

    expect(ledger.projects.map((e) => [e.label, e.totals.converted])).toEqual([
      ["Big - B", 50000],
    ]);
    expect(
      ledger.spinit.map((e) => [
        e.key,
        e.kind,
        e.spinit,
        e.label,
        e.totals.converted,
      ]),
    ).toEqual([
      [
        "spinit:untracked",
        "spinit",
        "untracked",
        "Spinit: untracked Copilot",
        15000,
      ],
      [
        "spinit:personal",
        "spinit",
        "personal",
        "Spinit: personal / internal Copilot",
        2500,
      ],
      [
        "spinit:accounts",
        "spinit",
        "accounts",
        "Spinit: unlinked, external and unknown accounts",
        2200,
      ],
    ]);
    expect(
      ledgerEntries(ledger)
        .map((e) => e.key)
        .slice(-3),
    ).toEqual(["spinit:untracked", "spinit:personal", "spinit:accounts"]);
    expect(ledger.totals.converted).toBe(74700);
  });

  test("credits outside any repository stay Unassigned, beside other such usage", () => {
    const [unattributed] = ledgerOf(meteredMonth()).unassigned;

    expect(unattributed.key).toBe("unassigned:unattributed");
    expect(unattributed.lines.map((l) => l.billingMode)).toEqual([
      "metered",
      "api",
    ]);
    expect(unattributed.totals.converted).toBe(5000);
  });

  test("an entry's people are its subjects: developers and accounts apart", () => {
    const ledger = ledgerOf(meteredMonth());
    const accounts = ledger.spinit.find((e) => e.spinit === "accounts");
    const untracked = ledger.spinit.find((e) => e.spinit === "untracked");

    expect(accounts.people.map((p) => [p.key, p.userId])).toEqual([
      ["account:copilot:2002", null],
      ["account:copilot:3003", null],
      ["unknown:copilot", null],
    ]);
    expect(untracked.people.map((p) => [p.key, p.userId])).toEqual([
      ["user:1", 1],
      ["user:2", 2],
    ]);
  });

  test("untracked credits get a solid segment, the rest of Spinit's one merged", () => {
    const segments = allocationSegments(ledgerOf(meteredMonth()));

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["unassigned", "No repository", null, 1],
      ["spinit", "Spinit: untracked Copilot", null, 1],
      ["spinit", "Spinit: other Copilot", null, 3],
    ]);
    expect(segments.slice(1).map((s) => [s.key, s.short, s.amount])).toEqual([
      ["unassigned:unattributed:usage", "No repo", 1000],
      ["spinit:untracked", "Untracked", 15000],
      [SPINIT_OTHER_KEY, "Other", 8700],
    ]);
    expect(
      segments[3].entries.map((e) => [e.label, e.totals.converted]),
    ).toEqual([
      ["Spinit: personal / internal Copilot", 2500],
      ["Spinit: unlinked, external and unknown accounts", 2200],
      ["Spinit: Copilot outside any repository", 4000],
    ]);
    const keys = segmentKeys(segments);
    expect(keys.get("spinit:personal")).toEqual([SPINIT_OTHER_KEY]);
    expect(keys.get("spinit:accounts")).toEqual([SPINIT_OTHER_KEY]);
    expect(keys.get("project:Big - B")).toEqual(["project:Big - B"]);
    // The ledger's row outside any repository stands for both of its parts.
    expect(keys.get("unassigned:unattributed")).toEqual([
      "unassigned:unattributed:usage",
      SPINIT_OTHER_KEY,
    ]);
    expect(segmentSteps(segments).has("spinit:untracked")).toBe(false);
  });

  test("the strip draws credits outside any repository as Spinit's, as the sentence counts them", () => {
    const ledger = ledgerOf(meteredMonth());
    const segments = allocationSegments(ledger);
    const share = spinitShare(ledger);
    const { unassigned, outside } = statementUnassigned(ledger);
    const drawn = (kind) =>
      segments
        .filter((s) => s.kind === kind)
        .reduce((sum, s) => sum + s.amount, 0);

    expect(drawn("spinit")).toBe(share.total);
    expect(
      unassigned.map((e) => [
        e.key,
        e.kind,
        e.lines.map((l) => l.billingMode),
        e.totals.converted,
      ]),
    ).toEqual([["unassigned:unattributed", "unassigned", ["api"], 1000]]);
    expect(drawn("unassigned")).toBe(1000);
    expect([
      outside.key,
      outside.kind,
      outside.spinit,
      outside.totals.converted,
    ]).toEqual(["unassigned:unattributed", "spinit", "unattributed", 4000]);
    expect(outside.totals.converted).toBe(share.unattributed);
    // The ledger itself keeps them under Unassigned.
    expect(ledger.unassigned[0].totals.converted).toBe(5000);
  });

  test("credits outside any repository alone keep a Spinit segment of their own", () => {
    const ledger = ledgerOf([
      line("Big - B", "300.00"),
      metered("project", null, "40.00"),
    ]);
    const segments = allocationSegments(ledger);

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["spinit", "Spinit: Copilot outside any repository", null, 1],
    ]);
    expect([segments[1].key, segments[1].short, segments[1].amount]).toEqual([
      SPINIT_UNATTRIBUTED_KEY,
      "Outside repos",
      4000,
    ]);
    expect(segments[1].entries[0].key).toBe("unassigned:unattributed");
    expect(segmentKeys(segments).get("unassigned:unattributed")).toEqual([
      SPINIT_UNATTRIBUTED_KEY,
    ]);
    expect(statementUnassigned(ledger).unassigned).toEqual([]);
  });

  test("one other kind of Spinit's credits keeps a segment of its own", () => {
    const segments = allocationSegments(
      ledgerOf([line("Big - B", "300.00"), metered("personal", null, "25.00")]),
    );

    expect(steps(segments)).toEqual([
      ["project", "Big - B", 1, 1],
      ["spinit", "Spinit: personal / internal Copilot", null, 1],
    ]);
    expect(segments[1].short).toBe("Personal");
  });

  test("what stays with Spinit counts credits outside any repository, not other such usage", () => {
    expect(spinitShare(ledgerOf(meteredMonth()))).toEqual({
      total: 23700,
      untracked: 15000,
      personal: 2500,
      accounts: 2200,
      unattributed: 4000,
    });
    expect(spinitShare(ledgerOf([line("Big - B", "300.00")]))).toEqual({
      total: 0,
      untracked: 0,
      personal: 0,
      accounts: 0,
      unattributed: 0,
    });
  });

  test("a Spinit line without an exchange rate leaves what stays with Spinit unknown", () => {
    const share = spinitShare(
      ledgerOf([
        metered("untracked", null, null, {
          missingRate: true,
          convertedAmount: null,
        }),
      ]),
    );

    expect(share.untracked).toBeNull();
    expect(share.total).toBeNull();
  });
});

/** A repository's code review credits, on `projectName` or, without one,
 * Unassigned as an unmapped repository. */
function review(name, projectName, sek) {
  const project =
    projectName === null ? null : { projectId: projectName, projectName };
  return line(null, sek, {
    subject: { kind: "repository", provider: "copilot", name },
    provider: "copilot",
    project,
    unassigned: project === null ? "unmappedRepository" : null,
    billingMode: "metered",
    billableCurrency: "USD",
    metered: {
      kind: "repository",
      credits: `${Math.round(Number(sek) * 10)}.00`,
      account: null,
      sku: "Copilot AI Credits",
      provisional: false,
      provisionalReasons: [],
      lowRatio: false,
    },
    usage: { ...usage, records: 0, apiEquivalentUsd: 0 },
  });
}

describe("code review of repositories", () => {
  test("a mapped repository joins its project; an unmapped one is Unassigned of its own, in bill order", () => {
    const ledger = ledgerOf([
      line(null, "10.00", { unassigned: "unattributed" }),
      review("gadget-web", null, "80.00"),
      line("Big - B", "300.00"),
      review("widget-api", "Big - B", "120.00"),
      line(null, "20.00"),
    ]);

    expect(ledger.projects.map((e) => [e.label, e.totals.converted])).toEqual([
      ["Big - B", 42000],
    ]);
    expect(
      ledger.unassigned.map((e) => [e.key, e.label, e.totals.converted]),
    ).toEqual([
      ["unassigned:unmapped", "Unmapped repositories", 2000],
      [
        "unassigned:unmappedRepository",
        "Code review: unmapped repositories",
        8000,
      ],
      ["unassigned:unattributed", "No repository", 1000],
    ]);
  });

  test("the statement and the strip count unmapped repositories as having no project, never as Spinit's", () => {
    const ledger = ledgerOf([
      line("Big - B", "300.00"),
      review("gadget-web", null, "80.00"),
    ]);

    const { unassigned, outside } = statementUnassigned(ledger);
    expect(unassigned.map((e) => e.unassigned)).toEqual(["unmappedRepository"]);
    expect(outside).toBeNull();
    expect(spinitShare(ledger).total).toBe(0);
    expect(
      allocationSegments(ledger).map((s) => [s.kind, s.short, s.amount]),
    ).toEqual([
      ["project", "Big", 30000],
      ["unassigned", "Code review", 8000],
    ]);
  });
});

describe("subjectsLabel", () => {
  const names = new Map([[1, "Ada Lovelace"]]);
  const people = (lines) => ledgerOf(lines).unassigned[0]?.people ?? [];

  test("names one subject, a repository by its code review", () => {
    expect(subjectsLabel(people([line(null, "1.00")]), names)).toBe(
      "Ada Lovelace",
    );
    expect(
      subjectsLabel(people([review("gadget-web", null, "1.00")]), names),
    ).toBe("Copilot code review · gadget-web");
  });

  test("counts people, accounts and repositories apart", () => {
    const [entry] = ledgerOf([
      review("widget-api", "Big - B", "3.00"),
      review("gadget-web", "Big - B", "2.00"),
      line("Big - B", "1.00"),
      line("Big - B", "1.00", { userId: 2 }),
      metered("project", "Big - B", "1.00", {
        subject: {
          kind: "providerAccount",
          provider: "copilot",
          accountId: 2002,
          login: "grace-hopper",
        },
      }),
    ]).projects;

    expect(subjectsLabel(entry.people, names)).toBe(
      "2 people, 1 account, 2 repositories",
    );
    expect(
      subjectsLabel(
        people([
          review("widget-api", null, "1.00"),
          review("gadget-web", null, "1.00"),
        ]),
        names,
      ),
    ).toBe("2 repositories");
  });
});

describe("peopleOf with a metered payer", () => {
  const overview = {
    developers: [
      { userId: 1, fullName: "Ada Lovelace", email: "ada@example.com" },
      { userId: 2, fullName: "Grace Hopper", email: "grace@example.com" },
    ],
    lines: meteredMonth(),
    subscriptions: [],
    personalUsage: [],
    adjustmentLines: [],
  };

  test("lists developers only, with their untracked credits apart", () => {
    const people = peopleOf(overview);

    expect(
      people.map((p) => [
        p.userId,
        p.totals?.converted,
        p.api,
        p.metered?.untracked.converted ?? null,
        p.projects,
        p.hasUnassigned,
      ]),
    ).toEqual([
      [1, 69500, true, 12000, ["Big - B"], true],
      [2, 3000, false, 3000, [], false],
    ]);
  });

  test("a developer without metered lines has no metered credits", () => {
    const [person] = peopleOf({
      ...overview,
      lines: [line("Big - B", "300.00")],
    });

    expect(person.metered).toBeNull();
    expect(person.api).toBe(true);
  });

  test("a developer with only adjustments to closed months is listed", () => {
    const people = peopleOf({
      ...overview,
      developers: [
        ...overview.developers,
        { userId: 3, fullName: "Alan Turing", email: "alan@example.com" },
      ],
      adjustmentLines: [
        { subject: { kind: "user", userId: 3 } },
        {
          subject: {
            kind: "repository",
            provider: "copilot",
            name: "widget-api",
          },
        },
      ],
    });

    expect(people.map((p) => [p.userId, p.name, p.totals, p.adjusted])).toEqual(
      [
        [1, "Ada Lovelace", people[0].totals, false],
        [2, "Grace Hopper", people[1].totals, false],
        [3, "Alan Turing", null, true],
      ],
    );
  });
});

describe("splitProjectName", () => {
  test.each([
    ["Norrsken Logistik - Kundportal", "Norrsken Logistik", "Kundportal"],
    ["Bo-Ek Bygg - Offertverktyg", "Bo-Ek Bygg", "Offertverktyg"],
    ["Customer - Phase 1 - Build", "Customer", "Phase 1 - Build"],
    ["Nordhamn Drift", "Nordhamn Drift", null],
    [" - Orphan", " - Orphan", null],
  ])("%s", (name, customer, project) => {
    expect(splitProjectName(name)).toEqual({ customer, project });
  });
});
