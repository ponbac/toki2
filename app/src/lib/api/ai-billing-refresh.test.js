import { describe, expect, test } from "bun:test";
import {
  PENDING_RATES_POLL_MS,
  whileRatesArePending,
  whileReadinessAwaitsRates,
  withoutFrozenFetches,
} from "./ai-billing-refresh.ts";

const close = { closeId: 9 };

describe("withoutFrozenFetches", () => {
  const julyUsd = { month: "2026-07", currency: "USD" };

  test("a closed month's rates being fetched when it closed are no longer pending", () => {
    const closed = {
      close,
      pendingRates: ["USD"],
      adjustedPendingRates: [julyUsd],
      missingRates: ["USD"],
    };

    expect(withoutFrozenFetches(closed)).toEqual({
      close,
      pendingRates: [],
      adjustedPendingRates: [],
      missingRates: ["USD"],
    });
    expect(
      whileRatesArePending({ state: { data: withoutFrozenFetches(closed) } }),
    ).toBe(false);
  });

  test("a month closed while only a corrected month's rate was fetched no longer polls", () => {
    const closed = {
      close,
      pendingRates: [],
      adjustedPendingRates: [julyUsd],
    };

    expect(withoutFrozenFetches(closed)).toEqual({
      close,
      pendingRates: [],
      adjustedPendingRates: [],
    });
    expect(
      whileRatesArePending({ state: { data: withoutFrozenFetches(closed) } }),
    ).toBe(false);
  });

  test("an open month keeps polling for its pending rates", () => {
    const open = {
      close: null,
      pendingRates: ["USD"],
      adjustedPendingRates: [],
    };

    expect(withoutFrozenFetches(open)).toBe(open);
    expect(whileRatesArePending({ state: { data: open } })).toBe(
      PENDING_RATES_POLL_MS,
    );
    expect(whileRatesArePending({ state: {} })).toBe(false);
  });

  test("an open month polls while a corrected month's rate is fetched, its own ready", () => {
    const open = {
      close: null,
      pendingRates: [],
      adjustedPendingRates: [julyUsd],
    };

    expect(withoutFrozenFetches(open)).toBe(open);
    expect(whileRatesArePending({ state: { data: open } })).toBe(
      PENDING_RATES_POLL_MS,
    );
    expect(
      whileRatesArePending({
        state: { data: { ...open, adjustedPendingRates: [] } },
      }),
    ).toBe(false);
  });
});

describe("whileReadinessAwaitsRates", () => {
  const status = (problems) => ({
    readiness: {
      checks: [
        { id: "pricing", status: "passed", problems: [] },
        { id: "exchangeRates", status: "failed", problems },
      ],
    },
  });

  test.each([
    {
      name: "a rate still being fetched",
      problem: { kind: "ratePending", currency: "USD" },
    },
    {
      name: "a corrected month's rate still being fetched",
      problem: {
        kind: "adjustedRatePending",
        month: "2026-07",
        currency: "USD",
      },
    },
  ])("polls while readiness awaits $name", ({ problem }) => {
    expect(
      whileReadinessAwaitsRates({ state: { data: status([problem]) } }),
    ).toBe(PENDING_RATES_POLL_MS);
  });

  test.each([
    {
      name: "a provisional rate",
      data: status([{ kind: "rateProvisional", currency: "USD" }]),
    },
    {
      name: "a corrected month's provisional rate",
      data: status([
        { kind: "adjustedRateProvisional", month: "2026-07", currency: "USD" },
      ]),
    },
    { name: "a month in progress or closed", data: { readiness: null } },
    { name: "no status yet", data: undefined },
  ])("stops for $name", ({ data }) => {
    expect(whileReadinessAwaitsRates({ state: { data } })).toBe(false);
  });
});
