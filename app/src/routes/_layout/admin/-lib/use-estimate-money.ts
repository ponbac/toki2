import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import type { AiExchangeRate } from "@/lib/api/queries/ai-admin";
import { aiUsageReportQueries } from "@/lib/api/queries/ai-usage-report";
import { USD_ESTIMATES, estimateMoney, type EstimateMoney } from "./estimates";
import { billUsdRate } from "./subscription-value";

/** How the estimates beneath a provider are shown; dollars without one. */
export const EstimateMoneyContext =
  React.createContext<EstimateMoney>(USD_ESTIMATES);

/** How to show estimates on this admin page. */
export function useEstimateMoney(): EstimateMoney {
  return React.useContext(EstimateMoneyContext);
}

/**
 * How a bill shows its estimates: in its currency at its own dollar rate, or,
 * for a bill without dollar amounts, at the display rate of the month that
 * ends `lastDay`. Fetches that display rate only when the bill has no rate.
 */
export function useBillEstimateMoney(
  bill:
    | Readonly<{
        billingCurrency: string;
        exchangeRates: readonly AiExchangeRate[];
        lastDay: string;
      }>
    | undefined,
): EstimateMoney {
  const billRate = bill
    ? billUsdRate(bill.exchangeRates, bill.billingCurrency)
    : null;
  const needsDisplay =
    bill !== undefined && billRate === null && bill.billingCurrency !== "USD";
  const displayRates = useQuery({
    ...aiUsageReportQueries.aiDisplayRates(bill?.lastDay),
    enabled: needsDisplay,
  });
  const displayRate =
    displayRates.data?.rates.find(
      (rate) => rate.currency === bill?.billingCurrency,
    )?.perUsd ?? null;
  const currency = bill?.billingCurrency ?? "USD";
  return React.useMemo(
    () => estimateMoney(currency, billRate, needsDisplay ? displayRate : null),
    [currency, billRate, needsDisplay, displayRate],
  );
}
