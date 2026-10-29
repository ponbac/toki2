import type { VolumeMeasure } from "../../-lib/adoption";
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

const tokenCount = new Intl.NumberFormat("en-US", {
  notation: "compact",
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});

/**
 * Tokens with exactly one decimal from a thousand up, such as `2.8B` or
 * `31.0B`, so their widths line up in a column and `2.8B` never reads as
 * `28B`.
 */
export function formatTokenCount(tokens: number): string {
  return tokens < 1000 ? String(Math.round(tokens)) : tokenCount.format(tokens);
}

/** An amount of `measure`: a cost, or tokens with their unit. */
export function formatAmount(
  value: number,
  measure: VolumeMeasure,
  money: EstimateMoney,
): string {
  return measure === "cost"
    ? formatCost(value, money)
    : `${formatTokenCount(value)} tokens`;
}
