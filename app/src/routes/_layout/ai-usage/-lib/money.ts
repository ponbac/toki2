import * as React from "react";
import { atomWithStorage } from "jotai/utils";
import type { AiCost, AiDisplayRates } from "@/lib/api/queries/ai-usage-report";

/** The currencies estimates can be shown in. */
export const DISPLAY_CURRENCIES = ["SEK", "USD", "EUR"] as const;
export type DisplayCurrency = (typeof DISPLAY_CURRENCIES)[number];

const DEFAULT_CURRENCY: DisplayCurrency = "SEK";

/** The currency last chosen, remembered in this browser. */
const displayCurrencyAtom = atomWithStorage<string>(
  "ai-usage-currency",
  DEFAULT_CURRENCY,
  undefined,
  { getOnInit: true },
);

function isDisplayCurrency(value: string): value is DisplayCurrency {
  return DISPLAY_CURRENCIES.some((currency) => currency === value);
}

/** The chosen currency; anything else in storage reads as the default. */
export function displayCurrency(stored: string): DisplayCurrency {
  return isDisplayCurrency(stored) ? stored : DEFAULT_CURRENCY;
}

export { displayCurrencyAtom };

/**
 * How estimates are shown: a currency, and what one US dollar is worth in it.
 * Estimates are priced in USD, so `perUsd` is 1 for USD.
 */
export type Money = Readonly<{ currency: DisplayCurrency; perUsd: number }>;

export const USD_MONEY: Money = { currency: "USD", perUsd: 1 };

/** How the page shows estimates. Without a provider, in USD as priced. */
export const MoneyContext = React.createContext<Money>(USD_MONEY);

export function useMoney(): Money {
  return React.useContext(MoneyContext);
}

/**
 * How to show estimates in `currency`: at its display rate, or in USD when
 * `rates` has none for it. Null while a rate that is needed is still loading.
 */
export function moneyFor(
  currency: DisplayCurrency,
  rates: AiDisplayRates | undefined,
  loading: boolean,
): Money | null {
  if (currency === "USD") {
    return USD_MONEY;
  }
  const rate = rates?.rates.find((rate) => rate.currency === currency);
  if (rate) {
    return { currency, perUsd: rate.perUsd };
  }
  return loading ? null : USD_MONEY;
}

/** SEK reads the Swedish way, `30 428 kr`; the others as the page's numbers. */
const LOCALES: Readonly<Record<DisplayCurrency, string>> = {
  SEK: "sv-SE",
  USD: "en-US",
  EUR: "en-US",
};

function formats(currency: DisplayCurrency) {
  const format = (minimumFractionDigits: number, maximumFractionDigits = 2) =>
    new Intl.NumberFormat(LOCALES[currency], {
      style: "currency",
      currency,
      minimumFractionDigits,
      maximumFractionDigits: Math.max(
        minimumFractionDigits,
        maximumFractionDigits,
      ),
    });
  return { exact: format(2), whole: format(0, 0), rate: format(2, 4) };
}

const FORMATS: Readonly<Record<DisplayCurrency, ReturnType<typeof formats>>> = {
  SEK: formats("SEK"),
  USD: formats("USD"),
  EUR: formats("EUR"),
};

/**
 * An amount already in the display currency, with hundredths: `$12.34` or
 * `12,34 kr`. SEK from 100 kr reads in whole kronor, `30 428 kr`. A positive
 * amount below a hundredth reads `<$0.01`.
 */
export function formatAmount(money: Money, amount: number): string {
  const { exact, whole } = FORMATS[money.currency];
  if (amount > 0 && amount < 0.005) {
    return `<${exact.format(0.01)}`;
  }
  return money.currency === "SEK" && amount >= 99.995
    ? whole.format(amount)
    : exact.format(amount);
}

/** A chart axis value in the display currency: `$0`, `$150`, `1 500 kr`. */
export function formatAxisAmount(money: Money, amount: number): string {
  return amount === 0 || amount >= 100
    ? FORMATS[money.currency].whole.format(amount)
    : formatAmount(money, amount);
}

/** A USD estimate, in the display currency. */
export function formatMoney(money: Money, usd: number): string {
  return formatAmount(money, usd * money.perUsd);
}

/** No money at all in `currency`, as in "never as $0". */
export function formatZero(currency: DisplayCurrency): string {
  return FORMATS[currency].whole.format(0);
}

/** What one US dollar is shown as, such as `9,42 kr`. */
export function formatRate(money: Money): string {
  return FORMATS[money.currency].rate.format(money.perUsd);
}

/**
 * An estimated cost. A partial cost reads as its known subtotal plus
 * "unknown", or only "unknown" when nothing was priced: unpriced usage is never
 * shown as zero. `short` uses "?" for charts.
 */
export function formatCost(money: Money, cost: AiCost, short = false): string {
  switch (cost.kind) {
    case "estimated":
      return formatMoney(money, cost.usd);
    case "partial": {
      const unknown = short ? "?" : "unknown";
      return cost.knownUsd > 0
        ? `${formatMoney(money, cost.knownUsd)} + ${unknown}`
        : short
          ? "?"
          : "Unknown";
    }
  }
}
