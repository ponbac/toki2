import type { AiBillingUsage } from "@/lib/api/queries/ai-admin";

/*
 * Formatting for the admin AI usage pages. USD figures are API-equivalent
 * estimates; fees are exact decimals in their own currency. Nothing converts
 * between currencies.
 */

const usd = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});
const compact = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
});
const whole = new Intl.NumberFormat("en-US");

/** Dollars with cents; a positive amount under a cent shows as `<$0.01`. */
export function formatUsd(amount: number): string {
  if (amount > 0 && amount < 0.005) {
    return "<$0.01";
  }
  return usd.format(amount);
}

/**
 * An API-equivalent estimate. Unpriced records have no known cost, which is
 * never shown as zero: wholly unpriced usage is "Unknown", partly unpriced
 * usage is its priced subtotal plus an unknown rest.
 */
export function formatEstimate(usage: AiBillingUsage): string {
  if (usage.unpricedRecords > 0 && usage.apiEquivalentUsd === 0) {
    return "Unknown";
  }
  const priced = formatUsd(usage.apiEquivalentUsd);
  return usage.unpricedRecords > 0 ? `${priced} + unknown` : priced;
}

/** An exact fee such as `1100.00` in its currency: `1,100.00 SEK`. */
export function formatFee(amount: string, currency: string): string {
  const [units, cents] = amount.split(".");
  return `${whole.format(Number(units))}.${cents} ${currency}`;
}

export function formatTokens(tokens: number): string {
  return compact.format(tokens);
}

export function formatCount(count: number): string {
  return whole.format(count);
}

/** `2026-08` as `August 2026`. */
export function monthLabel(month: string): string {
  const [year, monthNumber] = month.split("-").map(Number);
  return new Date(year, monthNumber - 1, 1).toLocaleDateString("en-US", {
    month: "long",
    year: "numeric",
  });
}

/** The month `delta` months after `month`, `YYYY-MM`. */
export function shiftMonth(month: string, delta: number): string {
  const [year, monthNumber] = month.split("-").map(Number);
  const shifted = new Date(year, monthNumber - 1 + delta, 1);
  return `${shifted.getFullYear()}-${String(shifted.getMonth() + 1).padStart(2, "0")}`;
}

/** The month before the current one in the browser's time zone, the one
 * usually being billed. */
export function previousMonth(): string {
  const now = new Date();
  return shiftMonth(
    `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`,
    -1,
  );
}

/** `2026-08-16` as `Aug 16`. */
export function shortDate(day: string): string {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

/** An instant's date in the browser's time zone, such as `Aug 21`. */
export function formatDateOf(instant: string): string {
  return new Date(instant).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

export const MONTH_PATTERN = /^\d{4}-(0[1-9]|1[0-2])$/;
