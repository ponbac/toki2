import { describe, expect, test } from "bun:test";
import { MIXED_BILLING, dailySeries, dayBilling } from "./daily.ts";

function row(day, provider, usd, extra = {}) {
  return {
    day,
    provider,
    billingProvider: null,
    model: "gpt-6.1-sol",
    machineId: "8d1f6a5e-2a4f-4a53-9a51-1d5b1f8f2c11",
    projectKey: null,
    project: null,
    billingMode: "api",
    subscriptionId: null,
    usage: {
      apiEquivalentUsd: usd,
      unpricedRecords: 0,
      records: 1,
      tokens: { input: 1, cacheRead: 0, cacheWrite: 0, output: 1, total: 2 },
    },
    ...extra,
  };
}

const ROWS = [
  row("2026-10-01", "codex", 4),
  row("2026-10-01", "opencode", 3, {
    billingProvider: "copilot",
    billingMode: "metered",
  }),
  row("2026-10-01", "copilot", 2, { billingMode: "metered" }),
  row("2026-10-02", "opencode", 1),
];

describe("dailySeries", () => {
  test("stacks a day by the provider who paid", () => {
    const [first, second] = dailySeries(
      ROWS,
      "2026-10-01",
      "2026-10-02",
      "payer",
    );

    expect([first.codex, first.copilot, first.opencode]).toEqual([4, 5, 0]);
    expect([second.copilot, second.opencode]).toEqual([0, 1]);
  });

  test("stacks a day by the tool that made the usage", () => {
    const [first] = dailySeries(ROWS, "2026-10-01", "2026-10-02", "provider");

    expect([first.codex, first.copilot, first.opencode]).toEqual([4, 2, 3]);
  });
});

describe("dayBilling", () => {
  const plans = new Map([[4, "ChatGPT Pro"]]);
  const metered = () => "GitHub Copilot credits";

  test("names how each group bills, and mixed groups as mixed", () => {
    const billing = dayBilling(
      [
        ...ROWS,
        row("2026-10-02", "codex", 1, {
          billingMode: "subscription",
          subscriptionId: 4,
        }),
        row("2026-10-02", "opencode", 1, {
          billingProvider: "copilot",
          billingMode: "metered",
        }),
      ],
      (r) => `${r.day}:${r.billingProvider ?? r.provider}`,
      plans,
      metered,
    );

    expect(Object.fromEntries(billing)).toEqual({
      "2026-10-01:codex": "API",
      "2026-10-01:copilot": "GitHub Copilot credits",
      "2026-10-02:opencode": "API",
      "2026-10-02:codex": "Subscription · ChatGPT Pro",
      "2026-10-02:copilot": "GitHub Copilot credits",
    });

    const byTool = dayBilling(
      [
        row("2026-10-02", "opencode", 1),
        row("2026-10-02", "opencode", 1, {
          billingProvider: "copilot",
          billingMode: "metered",
        }),
      ],
      (r) => `${r.day}:${r.provider}`,
      plans,
      metered,
    );
    expect(byTool.get("2026-10-02:opencode")).toBe(MIXED_BILLING);
  });

  test("a group stays mixed when a later row bills like its first", () => {
    const day = "2026-10-03";
    const billing = dayBilling(
      [
        row(day, "copilot", 1, { model: "gpt-6.1-sol" }),
        row(day, "copilot", 1, {
          model: "gpt-5.4",
          billingProvider: "codex",
          billingMode: "subscription",
          subscriptionId: 4,
        }),
        row(day, "copilot", 1, { model: "gpt-6-astra" }),
        row(day, "opencode", 1, {
          billingProvider: "copilot",
          billingMode: "metered",
        }),
        row(day, "opencode", 1, { model: "gpt-5.4" }),
        row(day, "opencode", 1, {
          model: "gpt-6-astra",
          billingProvider: "copilot",
          billingMode: "metered",
        }),
      ],
      (r) => `${r.day}:${r.provider}`,
      plans,
      metered,
    );

    expect(Object.fromEntries(billing)).toEqual({
      "2026-10-03:copilot": MIXED_BILLING,
      "2026-10-03:opencode": MIXED_BILLING,
    });
  });
});
