import { formatFee } from "@/lib/ai-subscriptions";
import type { AiBillingUsage } from "@/lib/api/queries/ai-admin";
import { formatEstimate, formatUsd } from "./format";

/*
 * API-price estimates on the admin pages, shown in the bill's currency so a
 * page never mixes it with dollars. Usage is priced in USD; it converts at the
 * bill's own dollar rate, the one its API lines were billed at, or at the
 * month's display rate when the bill has none. Until a rate is known the
 * estimates stay in dollars, as priced, rather than being guessed.
 */

/** Where an estimate's rate comes from. */
export type EstimateRateSource =
  /** The bill's dollar rate: what its API lines were converted at. */
  | "bill"
  /** The month's display rate, for a bill without dollar amounts. */
  | "display";

/** How estimates are shown: the currency, and what a dollar is worth in it. */
export type EstimateMoney =
  | Readonly<{ kind: "usd" }>
  | Readonly<{
      kind: "converted";
      currency: string;
      /** Units of `currency` per US dollar; always above zero. */
      perUsd: number;
      source: EstimateRateSource;
    }>;

/** Estimates as priced, in dollars: a USD bill, or no rate known yet. */
export const USD_ESTIMATES: EstimateMoney = { kind: "usd" };

/**
 * How a bill in `currency` shows its estimates: in `currency` at the bill's
 * dollar rate, else at the display rate, else in dollars. A USD bill needs no
 * rate. A rate that is not a positive number counts as none.
 */
export function estimateMoney(
  currency: string,
  billRate: number | null,
  displayRate: number | null,
): EstimateMoney {
  if (currency === "USD") {
    return USD_ESTIMATES;
  }
  const valid = (rate: number | null): rate is number =>
    rate !== null && Number.isFinite(rate) && rate > 0;
  if (valid(billRate)) {
    return { kind: "converted", currency, perUsd: billRate, source: "bill" };
  }
  if (valid(displayRate)) {
    return {
      kind: "converted",
      currency,
      perUsd: displayRate,
      source: "display",
    };
  }
  return USD_ESTIMATES;
}

/**
 * A dollar amount in `money`'s currency to hundredths, such as
 * `1 627,18 SEK`; a positive amount under a hundredth reads `<0,01 SEK`.
 */
export function formatEstimateAmount(
  usd: number,
  money: EstimateMoney,
): string {
  switch (money.kind) {
    case "usd":
      return formatUsd(usd);
    case "converted": {
      const amount = usd * money.perUsd;
      if (amount > 0 && amount < 0.005) {
        return `<${formatFee("0.01", money.currency)}`;
      }
      return formatFee(amount.toFixed(2), money.currency);
    }
  }
}

/**
 * Usage at API prices in `money`'s currency. Unpriced records have no known
 * cost, never zero: wholly unpriced usage is "Unknown", partly unpriced usage
 * its priced subtotal plus an unknown rest.
 */
export function formatEstimateIn(
  usage: AiBillingUsage,
  money: EstimateMoney,
): string {
  switch (money.kind) {
    case "usd":
      return formatEstimate(usage);
    case "converted": {
      if (usage.unpricedRecords > 0 && usage.apiEquivalentUsd === 0) {
        return "Unknown";
      }
      const priced = formatEstimateAmount(usage.apiEquivalentUsd, money);
      return usage.unpricedRecords > 0 ? `${priced} + unknown` : priced;
    }
  }
}

const wholeUnits = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });
const wholeKronor = new Intl.NumberFormat("sv-SE", {
  maximumFractionDigits: 0,
});

/** A chart axis value in `money`'s currency, in whole units: `1 500 SEK`. */
export function formatEstimateTick(usd: number, money: EstimateMoney): string {
  switch (money.kind) {
    case "usd":
      return formatUsd(usd);
    case "converted": {
      const amount = usd * money.perUsd;
      const format = money.currency === "SEK" ? wholeKronor : wholeUnits;
      return `${format.format(amount)} ${money.currency}`;
    }
  }
}

/**
 * The rate estimates are shown at, in words, such as `9.9713 SEK to the
 * dollar, the bill's rate`; null for estimates in dollars.
 */
export function estimateRateNote(money: EstimateMoney): string | null {
  switch (money.kind) {
    case "usd":
      return null;
    case "converted": {
      const rate = money.perUsd.toLocaleString("en-US", {
        maximumFractionDigits: 4,
      });
      const source =
        money.source === "bill" ? "the bill's rate" : "the month's average";
      return `${rate} ${money.currency} to the dollar, ${source}`;
    }
  }
}
