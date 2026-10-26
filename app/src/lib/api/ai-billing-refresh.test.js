import { describe, expect, test } from "bun:test";
import {
  PENDING_RATES_POLL_MS,
  readinessAwaitsRates,
  whileRatesArePending,
  whileReadinessAwaitsRates,
  withoutFrozenFetches,
} from "./ai-billing-refresh.ts";

const close = { closeId: 9 };

describe("withoutFrozenFetches", () => {
  test("a closed month's rates being fetched when it closed are no longer pending", () => {
    const closed = {
      close,
      pendingRates: ["USD"],
      missingRates: ["USD"],
    };

    expect(withoutFrozenFetches(closed)).toEqual({
      close,
      pendingRates: [],
      missingRates: ["USD"],
    });
    expect(
      whileRatesArePending({ state: { data: withoutFrozenFetches(closed) } }),
    ).toBe(false);
  });

  test("an open month keeps polling for its pending rates", () => {
    const open = { close: null, pendingRates: ["USD"] };

    expect(withoutFrozenFetches(open)).toBe(open);
    expect(whileRatesArePending({ state: { data: open } })).toBe(
      PENDING_RATES_POLL_MS,
    );
    expect(whileRatesArePending({ state: {} })).toBe(false);
  });
});

describe("readinessAwaitsRates", () => {
  const status = (problems) => ({
    readiness: {
      checks: [
        { id: "pricing", status: "passed", problems: [] },
        { id: "exchangeRates", status: "failed", problems },
      ],
    },
  });

  test("polls while a rate is still being fetched, and stops once it is not", () => {
    const pending = status([{ kind: "ratePending", currency: "USD" }]);
    const provisional = status([{ kind: "rateProvisional", currency: "USD" }]);

    expect(readinessAwaitsRates(pending)).toBe(true);
    expect(whileReadinessAwaitsRates({ state: { data: pending } })).toBe(
      PENDING_RATES_POLL_MS,
    );
    expect(readinessAwaitsRates(provisional)).toBe(false);
    expect(whileReadinessAwaitsRates({ state: { data: provisional } })).toBe(
      false,
    );
  });

  test("a month in progress or closed has no readiness to wait on", () => {
    expect(readinessAwaitsRates({ readiness: null })).toBe(false);
    expect(readinessAwaitsRates(undefined)).toBe(false);
  });
});
