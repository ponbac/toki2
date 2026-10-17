import * as React from "react";
import type { AiDisplayRates } from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import {
  DISPLAY_CURRENCIES,
  formatRate,
  type DisplayCurrency,
  type Money,
} from "../-lib/money";

const SYMBOLS: Readonly<Record<DisplayCurrency, string>> = {
  SEK: "kr",
  USD: "$",
  EUR: "€",
};

/**
 * The currency every estimate on the page is shown in, and under it the rate
 * that converts them. `money` is how estimates are shown right now: null while
 * the rate is not known, and USD when the chosen currency has no rate.
 */
export function CurrencySwitch({
  currency,
  onChange,
  money,
  rates,
  loading,
  failed,
  className,
}: {
  currency: DisplayCurrency;
  onChange: (currency: DisplayCurrency) => void;
  money: Money | null;
  rates: AiDisplayRates | undefined;
  loading: boolean;
  failed: boolean;
  className?: string;
}) {
  const buttons = React.useRef(new Map<DisplayCurrency, HTMLButtonElement>());
  const step = (from: DisplayCurrency, by: number) => {
    const index = DISPLAY_CURRENCIES.indexOf(from) + by;
    const next =
      DISPLAY_CURRENCIES[
        (index + DISPLAY_CURRENCIES.length) % DISPLAY_CURRENCIES.length
      ];
    if (next) {
      onChange(next);
      buttons.current.get(next)?.focus();
    }
  };

  return (
    <div className={cn("grid content-start gap-1.5", className)}>
      <div
        role="radiogroup"
        aria-label="Currency for costs"
        className="inline-flex gap-0.5 rounded-lg border border-border/70 bg-muted/40 p-0.5"
      >
        {DISPLAY_CURRENCIES.map((option) => {
          const checked = option === currency;
          return (
            <button
              key={option}
              ref={(button) => {
                if (button) {
                  buttons.current.set(option, button);
                } else {
                  buttons.current.delete(option);
                }
              }}
              type="button"
              role="radio"
              aria-checked={checked}
              tabIndex={checked ? 0 : -1}
              onClick={() => onChange(option)}
              onKeyDown={(event) => {
                const by =
                  event.key === "ArrowRight" || event.key === "ArrowDown"
                    ? 1
                    : event.key === "ArrowLeft" || event.key === "ArrowUp"
                      ? -1
                      : 0;
                if (by !== 0) {
                  event.preventDefault();
                  step(option, by);
                }
              }}
              className={cn(
                "inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-[13px] font-medium transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring",
                checked
                  ? "bg-background text-foreground shadow-xs ring-1 ring-inset ring-border"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <span
                aria-hidden
                className={cn(
                  "font-semibold",
                  checked ? "text-primary" : "opacity-75",
                )}
              >
                {SYMBOLS[option]}
              </span>
              {option}
            </button>
          );
        })}
      </div>
      <p
        className="min-h-4 text-xs tabular-nums text-muted-foreground"
        aria-live="polite"
      >
        <RateNote
          currency={currency}
          money={money}
          rates={rates}
          loading={loading}
          failed={failed}
        />
      </p>
    </div>
  );
}

/** Which rate converts the estimates, or why they are shown in USD. */
function RateNote({
  currency,
  money,
  rates,
  loading,
  failed,
}: {
  currency: DisplayCurrency;
  money: Money | null;
  rates: AiDisplayRates | undefined;
  loading: boolean;
  failed: boolean;
}) {
  if (currency === "USD") {
    return "As priced, no conversion";
  }
  if (money === null) {
    // Without usage on screen there is no rate to name yet.
    return loading ? `Loading the ${currency} rate…` : null;
  }
  const rate = rates?.rates.find((rate) => rate.currency === currency);
  if (money.currency !== currency || !rates || !rate) {
    return (
      <span className="text-foreground">
        {failed
          ? `Could not load the ${currency} rate.`
          : rates?.pending
            ? `Still fetching the ${currency} rate.`
            : `No ${currency} rate is published yet.`}{" "}
        Showing USD.
      </span>
    );
  }
  const month = monthName(rates.month);
  return (
    <>
      <span className="font-semibold text-foreground">
        1 USD = {formatRate(money)}
      </span>
      {rate.overridden
        ? `, set for ${month}’s bill`
        : rate.provisional
          ? `, ${month}’s average so far`
          : `, ${month}’s average`}
    </>
  );
}

/** `2026-10` as `October`, with the year unless it is this year. */
function monthName(month: string): string {
  const [year, monthNumber] = month.split("-").map(Number);
  const thisYear = year === new Date().getFullYear();
  return new Intl.DateTimeFormat("en-GB", {
    timeZone: "UTC",
    month: "long",
    year: thisYear ? undefined : "numeric",
  }).format(new Date(Date.UTC(year ?? 1970, (monthNumber ?? 1) - 1, 1)));
}
