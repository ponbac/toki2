import { useQuery } from "@tanstack/react-query";
import { useAtomValue } from "jotai/react";
import { aiUsageReportQueries } from "@/lib/api/queries/ai-usage-report";
import { displayCurrency, displayCurrencyAtom, moneyFor } from "./money";

/**
 * How to show the estimates of usage whose last local day is `through`, in
 * the chosen currency. Pass the `to` of the data on screen, as the server
 * resolved it, never the dates asked for: then a rate is only ever applied to
 * the usage of its own month, also while the next range is loading. `money`
 * is null while the rate is not known yet: without `through`, or while it
 * loads.
 */
export function useDisplayMoney(through: string | undefined) {
  const currency = displayCurrency(useAtomValue(displayCurrencyAtom));
  // Estimates are priced in USD: only another currency needs a rate.
  const ratesQuery = useQuery({
    ...aiUsageReportQueries.aiDisplayRates(through),
    enabled: currency !== "USD",
  });
  const waiting =
    currency !== "USD" && (through === undefined || ratesQuery.isPending);

  return {
    currency,
    money: moneyFor(currency, ratesQuery.data, waiting),
    rates: ratesQuery.data,
    fetching: ratesQuery.isFetching,
    failed: ratesQuery.isError,
  };
}
