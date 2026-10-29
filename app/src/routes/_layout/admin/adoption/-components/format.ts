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
 * A cost that may leave out usage without a known price: "Unknown" when it
 * has no priced usage at all, else the priced cost. The summary says once
 * how much usage costs leave out.
 */
export function formatKnownCost(
  usd: number,
  unpricedRecords: number,
  money: EstimateMoney,
): string {
  return unpricedRecords > 0 && usd === 0 ? "Unknown" : formatCost(usd, money);
}
