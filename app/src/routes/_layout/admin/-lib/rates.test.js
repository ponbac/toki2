import { describe, expect, test } from "bun:test";
import { rateChangeMessage } from "./rates.ts";

describe("rateChangeMessage", () => {
  const base = {
    monthLabel: "September 2026",
    currency: "USD",
    rate: "11.00",
    billingCurrency: "SEK",
    reset: false,
    adjustmentsPostOn: null,
  };

  test("an open month bills at the new rate", () => {
    expect(rateChangeMessage(base)).toBe(
      "September 2026 bills USD at 11.00 SEK",
    );
    expect(rateChangeMessage({ ...base, rate: "10.50", reset: true })).toBe(
      "USD is back at the Riksbank rate, 10.50 SEK",
    );
  });

  test("a closed month keeps its invoiced rate and the difference posts on the next open month", () => {
    const closed = { ...base, adjustmentsPostOn: "October 2026" };

    expect(rateChangeMessage(closed)).toBe(
      "September 2026 keeps the rate it was invoiced at. USD at 11.00 SEK posts any SEK difference as an adjustment on October 2026",
    );
    expect(rateChangeMessage({ ...closed, rate: "10.50", reset: true })).toBe(
      "September 2026 keeps the rate it was invoiced at. USD at the Riksbank rate, 10.50 SEK posts any SEK difference as an adjustment on October 2026",
    );
  });
});
