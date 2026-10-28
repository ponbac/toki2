import * as React from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import type { AiExchangeRate } from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import { listOf, monthLabel, shortDate } from "../../-lib/format";
import { parseRateInput, rateChangeMessage } from "../../-lib/rates";

function providerLabel(source: string): string {
  return source === "riksbank" ? "Riksbank" : source;
}

function fetchedLabel(fetched: NonNullable<AiExchangeRate["fetched"]>): string {
  const provider = providerLabel(fetched.source);
  return fetched.provisional
    ? `${provider} average of the days so far`
    : `${provider} monthly average`;
}

/** Warns that some amounts are not converted to the billing currency, because
 * a rate is missing or still being fetched. */
export function MissingRatesAlert({
  month,
  billingCurrency,
  missingRates,
  pendingRates,
}: {
  month: string;
  billingCurrency: string;
  missingRates: readonly string[];
  pendingRates: readonly string[];
}) {
  const missing = missingRates.filter((code) => !pendingRates.includes(code));
  return (
    <>
      {pendingRates.length > 0 && (
        <Alert role="status">
          <Loader2 className="size-4 animate-spin" />
          <AlertTitle>
            Fetching the {listOf(pendingRates, "conjunction")} exchange rate
            {pendingRates.length > 1 ? "s" : ""} for {monthLabel(month)}
          </AlertTitle>
          <AlertDescription>
            The {billingCurrency} figures update when the fetch finishes; this
            page checks again every few seconds.
          </AlertDescription>
        </Alert>
      )}
      {missing.length > 0 && (
        <Alert className="border-[#c98500]/50 dark:border-[#fab219]/50">
          <AlertTriangle className="size-4 text-[#c98500] dark:text-[#fab219]" />
          <AlertTitle>
            No {listOf(missing, "disjunction")} exchange rate for{" "}
            {monthLabel(month)}
          </AlertTitle>
          <AlertDescription>
            Amounts in {listOf(missing, "conjunction")} are not converted to{" "}
            {billingCurrency}: their {billingCurrency} figures and the totals
            that include them show &ldquo;No rate&rdquo;, never zero. Set a rate
            under Exchange rates on the AI billing overview, or bill from the
            CSV per currency.
          </AlertDescription>
        </Alert>
      )}
    </>
  );
}

/**
 * The month's exchange rates to the billing currency, each with its source,
 * whether it is provisional, the fetched rate kept beneath an override, and
 * an admin override: what the Exchange rates check opens to. It says first
 * what a missing or pending rate leaves unconverted. A closed month shows its
 * current rates, which an override edits, beside the ones it was invoiced at.
 */
export function ExchangeRatesPanel({
  month,
  billingCurrency,
  rates,
  missingRates,
  pendingRates,
  closed = null,
  disabled = false,
}: {
  month: string;
  billingCurrency: string;
  /** The rates an override edits: a closed month's current ones. */
  rates: readonly AiExchangeRate[];
  missingRates: readonly string[];
  pendingRates: readonly string[];
  /** A closed month: the rates its bill keeps, and where a changed rate's
   * SEK difference posts. */
  closed?: Readonly<{
    invoicedRates: readonly AiExchangeRate[];
    adjustmentsPostOn: string;
  }> | null;
  disabled?: boolean;
}) {
  const byCurrency = new Map(rates.map((rate) => [rate.currency, rate]));
  const invoiced = new Map(
    (closed?.invoicedRates ?? []).map((rate) => [rate.currency, rate]),
  );
  const currencies = [
    ...new Set([...byCurrency.keys(), ...invoiced.keys(), ...missingRates]),
  ].sort();
  const missing = missingRates.filter(
    (code) => !pendingRates.includes(code) && !byCurrency.has(code),
  );

  return (
    <div className="flex flex-col gap-3">
      {closed && (
        <p className="text-sm">
          {monthLabel(month)} is closed: its bill keeps the rate it closed with.
          A new rate posts the {billingCurrency} difference as an adjustment on{" "}
          {monthLabel(closed.adjustmentsPostOn)}.
        </p>
      )}
      {!closed && missing.length > 0 && (
        <p className="text-sm">
          Amounts in {listOf(missing, "conjunction")} are not converted to{" "}
          {billingCurrency}: their {billingCurrency} figures and the totals that
          include them show &ldquo;No rate&rdquo;, never zero. Set a rate below,
          or bill from the CSV per currency.
        </p>
      )}
      {pendingRates.length > 0 && (
        <p className="text-sm" role="status">
          The {billingCurrency} figures update when the fetch finishes; this
          page checks again every few seconds.
        </p>
      )}
      <p className="text-sm text-muted-foreground">
        {billingCurrency} per unit, the Riksbank&apos;s average for{" "}
        {monthLabel(month)}. A month in progress uses the average of the days
        published so far, which is provisional and refreshed after each
        publication. An override applies instead of the fetched rate, which is
        kept and applies again on reset.
      </p>
      <div className="rounded-lg border border-border/60">
        {currencies.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">
            Everything this month is billed in {billingCurrency}; no rate is
            needed.
          </p>
        ) : (
          <ul className="divide-y divide-border/60">
            {currencies.map((currency) => (
              <RateRow
                key={`${month}-${currency}`}
                month={month}
                currency={currency}
                billingCurrency={billingCurrency}
                rate={byCurrency.get(currency)}
                invoiced={closed ? (invoiced.get(currency) ?? null) : undefined}
                adjustmentsPostOn={closed?.adjustmentsPostOn ?? null}
                pending={pendingRates.includes(currency)}
                disabled={disabled}
              />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function RateRow({
  month,
  currency,
  billingCurrency,
  rate,
  invoiced,
  adjustmentsPostOn,
  pending,
  disabled,
}: {
  month: string;
  currency: string;
  billingCurrency: string;
  rate: AiExchangeRate | undefined;
  /** A closed month's rate as invoiced (null: invoiced without one);
   * undefined for an open month. */
  invoiced?: AiExchangeRate | null;
  /** A closed month: where a changed rate's difference posts. */
  adjustmentsPostOn: string | null;
  pending: boolean;
  disabled: boolean;
}) {
  const [editing, setEditing] = React.useState(false);
  const [input, setInput] = React.useState(rate?.rate ?? "");
  const message = (applied: string, reset: boolean) =>
    rateChangeMessage({
      monthLabel: monthLabel(month),
      currency,
      rate: applied,
      billingCurrency,
      reset,
      adjustmentsPostOn:
        adjustmentsPostOn === null ? null : monthLabel(adjustmentsPostOn),
    });
  const override = aiAdminMutations.useOverrideExchangeRate({
    onSuccess: (saved) => {
      toast.success(message(saved.rate, false));
      setEditing(false);
    },
    onError: apiErrorToast(`Failed to set the ${currency} rate`),
  });
  const reset = aiAdminMutations.useResetExchangeRate({
    onSuccess: (fetched) =>
      fetched
        ? toast.success(message(fetched.rate, true))
        : toast.warning(
            `Override removed; no Riksbank ${currency} rate is available yet`,
          ),
    onError: apiErrorToast(`Failed to reset the ${currency} rate`),
  });
  const parsed = parseRateInput(input);
  const inputId = `rate-${currency}`;
  const errorId = `${inputId}-error`;
  const fetched = rate?.fetched ?? null;
  const overridden = rate?.override ?? null;

  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-3 py-3">
      <div className="min-w-0 flex-1 basis-56">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 tabular-nums">
          <span className="font-medium">{currency}</span>
          {rate ? (
            <span>
              <span className="font-semibold">{rate.rate}</span>{" "}
              <span className="text-xs text-muted-foreground">
                {billingCurrency} per {currency}
              </span>
            </span>
          ) : (
            !pending && <StatusBadge tone="warning" label="Missing" />
          )}
          {pending ? (
            <StatusBadge
              tone="neutral"
              label={rate ? "Refreshing…" : "Fetching…"}
            />
          ) : rate === undefined ? null : rate.provisional ? (
            <StatusBadge tone="warning" label="Provisional" />
          ) : (
            <StatusBadge tone="good" label="Final" />
          )}
        </div>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {overridden ? (
            <>
              <span className="text-foreground">Admin override.</span> Set{" "}
              {shortDate(overridden.overriddenOn)}
              {overridden.overriddenBy ? ` by ${overridden.overriddenBy}` : ""}
              {fetched
                ? ` · ${providerLabel(fetched.source)}: ${fetched.rate}`
                : ""}
            </>
          ) : fetched ? (
            <>
              <span className="text-foreground">{fetchedLabel(fetched)}.</span>{" "}
              Through {shortDate(fetched.observedThrough)} · fetched{" "}
              {shortDate(fetched.fetchedOn)}
            </>
          ) : pending ? (
            "Asking the Riksbank"
          ) : (
            "Not published, or the Riksbank could not be reached"
          )}
        </p>
        {invoiced !== undefined && (
          <p className="mt-0.5 text-xs tabular-nums">
            {invoiced === null ? (
              "Invoiced without a rate."
            ) : (
              <>
                Invoiced at{" "}
                <span className="font-semibold">{invoiced.rate}</span>
                {rate && rate.rate !== invoiced.rate && (
                  <span className="text-muted-foreground">
                    {" "}
                    · now {rate.rate}: the difference posts as an adjustment
                  </span>
                )}
              </>
            )}
          </p>
        )}
      </div>
      <div className="ml-auto">
        {editing ? (
          <form
            className="flex flex-col items-end gap-1"
            onSubmit={(event) => {
              event.preventDefault();
              if (!disabled && "rate" in parsed) {
                override.mutate({ month, currency, rate: parsed.rate });
              }
            }}
          >
            <div className="flex flex-wrap items-center justify-end gap-2">
              <label htmlFor={inputId} className="sr-only">
                {billingCurrency} per {currency}
              </label>
              <Input
                id={inputId}
                inputMode="decimal"
                autoFocus
                value={input}
                disabled={disabled}
                onChange={(event) => setInput(event.target.value)}
                className="h-8 w-32 text-right tabular-nums"
                aria-invalid={"error" in parsed}
                aria-describedby={"error" in parsed ? errorId : undefined}
              />
              <Button
                type="submit"
                size="sm"
                disabled={disabled || "error" in parsed || override.isPending}
              >
                {override.isPending ? "Saving..." : "Save"}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setEditing(false)}
              >
                Cancel
              </Button>
            </div>
            {"error" in parsed && input !== "" && (
              <p
                id={errorId}
                className="text-xs text-[#c98500] dark:text-[#fab219]"
              >
                {parsed.error}
              </p>
            )}
          </form>
        ) : (
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={disabled}
              onClick={() => {
                setInput(rate?.rate ?? "");
                setEditing(true);
              }}
            >
              {overridden ? "Change" : "Override"}
            </Button>
            {overridden && (
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={disabled || reset.isPending}
                onClick={() => reset.mutate({ month, currency })}
              >
                {reset.isPending ? "Resetting..." : "Reset to Riksbank"}
              </Button>
            )}
          </div>
        )}
      </div>
    </li>
  );
}
