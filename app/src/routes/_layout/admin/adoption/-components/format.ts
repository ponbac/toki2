import {
  formatEstimateAmount,
  formatEstimateTick,
  type EstimateMoney,
} from "../../-lib/estimates";

/**
 * A cost in whole units of its currency, or to hundredths while under ten of
 * them, so a small cost never reads as nothing.
 */
export function formatCost(usd: number, money: EstimateMoney): string {
  const units = money.kind === "converted" ? usd * money.perUsd : usd;
  return units >= 10
    ? formatEstimateTick(usd, money)
    : formatEstimateAmount(usd, money);
}

/**
 * A cost that may hold usage without a known price, which is never shown as
 * zero: wholly unpriced usage is "Unknown", partly unpriced usage its priced
 * subtotal plus an unknown rest.
 */
export function formatKnownCost(
  usd: number,
  unpricedRecords: number,
  money: EstimateMoney,
): string {
  if (unpricedRecords > 0 && usd === 0) return "Unknown";
  const priced = formatCost(usd, money);
  return unpricedRecords > 0 ? `${priced} + unknown` : priced;
}
