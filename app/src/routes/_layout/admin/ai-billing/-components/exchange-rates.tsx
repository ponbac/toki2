import * as React from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { apiErrorToast } from "@/lib/api/errors";
import { aiAdminMutations } from "@/lib/api/mutations/ai-admin";
import type { AiExchangeRate } from "@/lib/api/queries/ai-admin";
import { StatusBadge } from "../../-components/status-badge";
import { monthLabel, shortDate } from "../../-lib/format";
import { parseRateInput } from "../../-lib/rates";

/** Currency codes as a sentence: `EUR, GBP and USD` or `EUR, GBP or USD`. */
function listOf(
  codes: readonly string[],
  type: "conjunction" | "disjunction",
): string {
  return new Intl.ListFormat("en", { style: "long", type }).format(codes);
}

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
 * an admin override.
 */
export function ExchangeRatesPanel({
  month,
  billingCurrency,
  rates,
  missingRates,
  pendingRates,
  disabled = false,
}: {
  month: string;
  billingCurrency: string;
  rates: readonly AiExchangeRate[];
  missingRates: readonly string[];
  pendingRates: readonly string[];
  disabled?: boolean;
}) {
  const byCurrency = new Map(rates.map((rate) => [rate.currency, rate]));
  const currencies = [
    ...new Set([...byCurrency.keys(), ...missingRates]),
  ].sort();

  return (
    <section
      aria-labelledby="exchange-rates-heading"
      className="flex flex-col gap-2"
    >
      <div>
        <h2 id="exchange-rates-heading" className="text-lg font-semibold">
          Exchange rates
        </h2>
        <p className="text-sm text-muted-foreground">
          {billingCurrency} per unit, the Riksbank&apos;s average for{" "}
          {monthLabel(month)}. A month in progress uses the average of the days
          published so far, which is provisional and refreshed after each
          publication. An override applies instead of the fetched rate, which is
          kept and applies again on reset.
        </p>
      </div>
      <div className="rounded-xl border border-border/60 bg-card/60">
        {currencies.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">
            Everything this month is billed in {billingCurrency}; no rate is
            needed.
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Currency</TableHead>
                <TableHead className="text-right">Rate</TableHead>
                <TableHead>Source</TableHead>
                <TableHead>State</TableHead>
                <TableHead className="text-right">Override</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {currencies.map((currency) => (
                <RateRow
                  key={`${month}-${currency}`}
                  month={month}
                  currency={currency}
                  billingCurrency={billingCurrency}
                  rate={byCurrency.get(currency)}
                  pending={pendingRates.includes(currency)}
                  disabled={disabled}
                />
              ))}
            </TableBody>
          </Table>
        )}
      </div>
    </section>
  );
}

function RateRow({
  month,
  currency,
  billingCurrency,
  rate,
  pending,
  disabled,
}: {
  month: string;
  currency: string;
  billingCurrency: string;
  rate: AiExchangeRate | undefined;
  pending: boolean;
  disabled: boolean;
}) {
  const [editing, setEditing] = React.useState(false);
  const [input, setInput] = React.useState(rate?.rate ?? "");
  const override = aiAdminMutations.useOverrideExchangeRate({
    onSuccess: (saved) => {
      toast.success(
        `${monthLabel(month)} bills ${currency} at ${saved.rate} ${billingCurrency}`,
      );
      setEditing(false);
    },
    onError: apiErrorToast(`Failed to set the ${currency} rate`),
  });
  const reset = aiAdminMutations.useResetExchangeRate({
    onSuccess: (fetched) =>
      fetched
        ? toast.success(
            `${currency} is back at the Riksbank rate, ${fetched.rate} ${billingCurrency}`,
          )
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
    <TableRow>
      <TableCell className="font-medium">{currency}</TableCell>
      <TableCell className="text-right tabular-nums">
        {rate ? (
          <>
            <span className="font-semibold">{rate.rate}</span>{" "}
            <span className="text-xs text-muted-foreground">
              {billingCurrency} per {currency}
            </span>
          </>
        ) : pending ? (
          <StatusBadge tone="neutral" label="Fetching…" />
        ) : (
          <StatusBadge tone="warning" label="Missing" />
        )}
      </TableCell>
      <TableCell>
        {overridden ? (
          <>
            <div className="text-sm">Admin override</div>
            <div className="text-xs text-muted-foreground">
              Set {shortDate(overridden.overriddenOn)}
              {overridden.overriddenBy ? ` by ${overridden.overriddenBy}` : ""}
              {fetched
                ? ` · ${providerLabel(fetched.source)}: ${fetched.rate}`
                : ""}
            </div>
          </>
        ) : fetched ? (
          <>
            <div className="text-sm">{fetchedLabel(fetched)}</div>
            <div className="text-xs text-muted-foreground">
              Through {shortDate(fetched.observedThrough)} · fetched{" "}
              {shortDate(fetched.fetchedOn)}
            </div>
          </>
        ) : (
          <span className="text-sm text-muted-foreground">
            {pending
              ? "Asking the Riksbank"
              : "Not published, or the Riksbank could not be reached"}
          </span>
        )}
      </TableCell>
      <TableCell>
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
      </TableCell>
      <TableCell className="text-right">
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
            <div className="flex items-center gap-2">
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
          <div className="flex items-center justify-end gap-2">
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
      </TableCell>
    </TableRow>
  );
}
