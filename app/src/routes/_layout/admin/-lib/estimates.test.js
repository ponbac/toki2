import { describe, expect, test } from "bun:test";
import {
  estimateMoney,
  estimateRateNote,
  formatEstimateIn,
  formatEstimateTick,
} from "./estimates.ts";

function usage(usd, unpricedRecords = 0) {
  return {
    apiEquivalentUsd: usd,
    unpricedRecords,
    records: unpricedRecords + 1,
    tokens: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, total: 0 },
  };
}

describe("estimateMoney", () => {
  test("the bill's own dollar rate wins over the month's display rate", () => {
    expect(estimateMoney("SEK", 9.9713, 10.2)).toEqual({
      kind: "converted",
      currency: "SEK",
      perUsd: 9.9713,
      source: "bill",
    });
    expect(estimateMoney("SEK", null, 10.2)).toEqual({
      kind: "converted",
      currency: "SEK",
      perUsd: 10.2,
      source: "display",
    });
  });

  test("without a usable rate, or for a USD bill, estimates stay in dollars", () => {
    expect(estimateMoney("SEK", null, null)).toEqual({ kind: "usd" });
    expect(estimateMoney("SEK", 0, Number.NaN)).toEqual({ kind: "usd" });
    expect(estimateMoney("USD", null, null)).toEqual({ kind: "usd" });
  });
});

describe("formatEstimateIn", () => {
  const sek = estimateMoney("SEK", 10, null);

  test("converts to the bill's currency, never mixing in dollars", () => {
    expect(formatEstimateIn(usage(162.718), sek)).toBe("1\u00a0627,18 SEK");
    expect(formatEstimateIn(usage(0.0001), sek)).toBe("<0,01 SEK");
  });

  test("unpriced usage is unknown, never zero", () => {
    expect(formatEstimateIn(usage(0, 3), sek)).toBe("Unknown");
    expect(formatEstimateIn(usage(2.5, 3), sek)).toBe("25,00 SEK + unknown");
  });

  test("in dollars while no rate is known", () => {
    expect(formatEstimateIn(usage(2.5), { kind: "usd" })).toBe("$2.50");
  });
});

test("axis ticks and the rate note name the currency and its source", () => {
  const money = estimateMoney("SEK", 9.97131, null);

  expect(formatEstimateTick(150.4, money)).toBe("1\u00a0500 SEK");
  expect(estimateRateNote(money)).toBe(
    "9.9713 SEK to the dollar, the bill's rate",
  );
  expect(estimateRateNote({ kind: "usd" })).toBeNull();
});
