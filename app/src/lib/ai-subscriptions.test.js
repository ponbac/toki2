import { describe, expect, test } from "bun:test";
import { formatFee } from "./ai-subscriptions.ts";

const NBSP = " ";

describe("formatFee", () => {
  test.each([
    { amount: "1100.00", currency: "USD", shown: "1,100.00 USD" },
    { amount: "1.50", currency: "USD", shown: "1.50 USD" },
    { amount: "0.00", currency: "USD", shown: "0.00 USD" },
    { amount: "1234.05", currency: "SEK", shown: `1${NBSP}234,05 SEK` },
    // An adjustment's credit keeps its sign, as a minus.
    { amount: "-1.50", currency: "USD", shown: "−1.50 USD" },
    { amount: "-0.05", currency: "USD", shown: "−0.05 USD" },
    { amount: "-1234.05", currency: "SEK", shown: `−1${NBSP}234,05 SEK` },
    { amount: "-1100.00", currency: "EUR", shown: "−1,100.00 EUR" },
    // A zero is never signed, however it was computed.
    { amount: "-0.00", currency: "SEK", shown: "0,00 SEK" },
  ])("$amount $currency reads $shown", ({ amount, currency, shown }) => {
    expect(formatFee(amount, currency)).toBe(shown);
  });
});
