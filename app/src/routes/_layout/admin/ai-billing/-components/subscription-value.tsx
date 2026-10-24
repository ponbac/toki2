import { barX, defineChart, ruleX } from "@tanstack/charts";
import { Chart } from "@tanstack/charts/react/tooltip";
import { scaleBand } from "@tanstack/charts/scales/band";
import { scaleLinear } from "@tanstack/charts/scales/linear";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { AlertTriangle } from "lucide-react";
import { useMemo, useState } from "react";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { formatFee } from "@/lib/ai-subscriptions";
import type { AiBillingOverview } from "@/lib/api/queries/ai-admin";
import type { AiProvider } from "@/lib/api/queries/ai-usage";
import { aiUsageReportQueries } from "@/lib/api/queries/ai-usage-report";
import { USAGE_CHART_THEME, USAGE_CHART_TOOLTIP } from "@/lib/chart-theme";
import {
  PENDING_RATE_LABEL,
  formatAmount,
  formatConverted,
} from "../../-lib/billing";
import { formatEstimate, formatUsd } from "../../-lib/format";
import {
  apiBilledUsage,
  billUsdRate,
  formatMultiple,
  formatMultipleTick,
  multipleAxis,
  multipleExponent,
  planFeeRange,
  subscriptionValues,
  summarizeValues,
  verdict,
  type MultipleAxis,
  type SubscriptionValue,
} from "../../-lib/subscription-value";

/** Usage worth more than the fee: the page's one series colour. */
const ABOVE_COLOR = "var(--ai-series-1)";
/** A fee that cost more than its usage, in the warning status colour. */
const BELOW_COLOR = "#c98500";
/** A floor below the fee: unpriced usage may yet lift it above. */
const OPEN_COLOR = "var(--viz-other)";
const GRID_COLOR = "hsl(var(--border) / 0.6)";

const FILLS = {
  above: ABOVE_COLOR,
  below: BELOW_COLOR,
  open: OPEN_COLOR,
} as const;

const ROW = "row";
/** API usage billing less than this, in hundredths, folds away. */
const SMALL_API_USAGE = 100;
/** Fewer small entries than this stay in view. */
const FOLD_AT = 3;
const BAR_HEIGHT = 22;

/**
 * Who and what bought it, the multiple on a shared axis, then the figures.
 * On a narrow screen the bar and the figures drop beneath the name.
 */
const ROW_GRID =
  "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 [grid-template-areas:'who_worth'_'plot_plot'_'sums_sums'] sm:grid-cols-[12rem_minmax(0,1fr)_15rem] sm:gap-y-0 sm:[grid-template-areas:'who_plot_worth'_'who_plot_sums']";

type Amounts = Readonly<{
  currency: string;
  /** No rate yet, rather than none at all. */
  ratePending: boolean;
}>;

/**
 * What each subscription bought in the month: the usage it paid for at API
 * prices as a multiple of its pro-rated fee, both in the billing currency, and
 * what bills at API prices without a subscription. The subscriptions table
 * below is its table view.
 */
export function SubscriptionValueChart({
  overview,
}: {
  overview: AiBillingOverview;
}) {
  const { billingCurrency, subscriptions } = overview;
  const billRate = billUsdRate(overview.exchangeRates, billingCurrency);
  // A bill without dollar amounts has no dollar rate; the month's display
  // rate stands in, as it does for a developer's own estimates.
  const displayRates = useQuery({
    ...aiUsageReportQueries.aiDisplayRates(overview.lastDay),
    enabled: billRate === null && subscriptions.length > 0,
  });
  const usdRate =
    billRate ??
    displayRates.data?.rates.find((rate) => rate.currency === billingCurrency)
      ?.perUsd ??
    null;
  // The request may be in flight, or answered while the server still fetches.
  const ratePending =
    billRate === null &&
    subscriptions.length > 0 &&
    (displayRates.isPending || displayRates.data?.pending === true);
  const amounts = useMemo(
    (): Amounts => ({ currency: billingCurrency, ratePending }),
    [billingCurrency, ratePending],
  );

  const { values, summary, axis, apiUsage } = useMemo(() => {
    const values = subscriptionValues(
      subscriptions,
      overview.developers,
      usdRate,
    );
    return {
      values,
      summary: summarizeValues(values),
      axis: multipleAxis(values),
      apiUsage: apiBilledUsage(overview.lines, overview.developers),
    };
  }, [subscriptions, overview.developers, overview.lines, usdRate]);

  const [showSmall, setShowSmall] = useState(false);
  // Usage that bills under one unit each folds away, unless it is unknown.
  const small = apiUsage.filter(
    (usage) =>
      usage.totals.converted !== null &&
      usage.totals.converted < SMALL_API_USAGE &&
      usage.totals.unknownRecords === 0,
  );
  const folded = small.length >= FOLD_AT && !showSmall;
  const listed = folded
    ? apiUsage.filter((usage) => !small.includes(usage))
    : apiUsage;

  if (subscriptions.length === 0 && apiUsage.length === 0) {
    return null;
  }

  const atLeast = summary.atLeast ? "at least " : "";
  const undecided = summary.measured - summary.decided;
  return (
    <section
      aria-label="What subscriptions bought, and what billed at API prices"
      className="flex flex-col gap-4 rounded-xl border border-border/60 bg-card/60 p-4"
    >
      {subscriptions.length > 0 && (
        <>
          <div>
            <h3 className="text-sm font-semibold">
              What each subscription bought
            </h3>
            <p className="text-xs text-muted-foreground">
              The usage a plan paid for at API prices, as a multiple of its
              pro-rated fee over the same days. Usage marked as not client work
              is not counted.
              {usdRate !== null &&
                billingCurrency !== "USD" &&
                ` Estimates are shown in ${billingCurrency} at ${usdRate.toLocaleString("en-US", { maximumFractionDigits: 4 })} to the dollar.`}
            </p>
          </div>

          {summary.measured > 0 && (
            <dl className="grid grid-cols-2 gap-x-4 gap-y-3 lg:grid-cols-4">
              <Stat
                label="Fees"
                value={formatAmount(summary.fees, billingCurrency)}
                note={
                  summary.measured === summary.plans
                    ? `${summary.plans} plans, pro-rated`
                    : `${summary.measured} of ${summary.plans} plans, pro-rated`
                }
              />
              <Stat
                label="Their usage at API prices"
                value={formatAmount(summary.usage, billingCurrency)}
                note={
                  summary.atLeast
                    ? "priced records only; some are unpriced"
                    : "of the same plans"
                }
              />
              <Stat
                label="Overall"
                value={
                  summary.overall === null
                    ? "—"
                    : `${summary.atLeast ? "≥ " : ""}${formatMultiple(summary.overall)}`
                }
                note={`usage is ${atLeast}this many times the fees`}
              />
              <Stat
                label="Below 1×"
                value={`${summary.below} of ${summary.decided}`}
                note={
                  undecided > 0
                    ? `plans cost more than they bought; ${undecided} more cannot be told`
                    : "plans cost more than they bought"
                }
              />
            </dl>
          )}

          <div className="flex flex-col">
            <div className={ROW_GRID} aria-hidden>
              <div className="relative h-8 text-[10px] text-muted-foreground [grid-area:plot]">
                <span
                  className="absolute top-0 hidden whitespace-nowrap pr-1.5 sm:block"
                  style={{ right: `${100 - tickPosition(0, axis)}%` }}
                >
                  ◂ fee cost more
                </span>
                <span
                  className="absolute top-0 hidden whitespace-nowrap pl-1.5 sm:block"
                  style={{ left: `${tickPosition(0, axis)}%` }}
                >
                  usage worth more ▸
                </span>
                {axis.ticks.map((tick) => (
                  <span
                    key={tick}
                    className="absolute bottom-0 -translate-x-1/2 whitespace-nowrap tabular-nums"
                    style={{ left: `${tickPosition(tick, axis)}%` }}
                  >
                    {formatMultipleTick(tick)}
                  </span>
                ))}
              </div>
            </div>
            {values.map((value) => (
              <ValueRow
                key={value.subscription.subscriptionId}
                value={value}
                axis={axis}
                amounts={amounts}
              />
            ))}
          </div>

          <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
            <li className="flex items-center gap-1.5">
              <span
                aria-hidden
                className="size-2.5 shrink-0 rounded-[2px]"
                style={{ backgroundColor: ABOVE_COLOR }}
              />
              Usage worth more than the fee
            </li>
            <li className="flex items-center gap-1.5">
              <span
                aria-hidden
                className="size-2.5 shrink-0 rounded-[2px]"
                style={{ backgroundColor: BELOW_COLOR }}
              />
              <WarningIcon />
              Fee cost more than the usage
            </li>
            {values.some(
              (value) =>
                value.worth.kind === "multiple" &&
                verdict(value.worth) === "open",
            ) && (
              <li className="flex items-center gap-1.5">
                <span
                  aria-hidden
                  className="size-2.5 shrink-0 rounded-[2px]"
                  style={{ backgroundColor: OPEN_COLOR }}
                />
                At least this much: unpriced usage may lift it past the fee
              </li>
            )}
            <li>1× is break-even, on a logarithmic scale</li>
          </ul>
        </>
      )}

      {apiUsage.length > 0 && (
        <div
          className={
            subscriptions.length > 0 ? "border-t border-border/50 pt-3" : ""
          }
        >
          <h3 className="text-sm font-semibold">Billed at API prices</h3>
          <p className="mb-2 text-xs text-muted-foreground">
            Usage no subscription paid for.
            {planHints(
              overview,
              apiUsage.map((usage) => usage.provider),
            )}
          </p>
          <dl className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-1 text-sm">
            {listed.map((usage) => (
              <div
                key={`${usage.userId}:${usage.provider}`}
                className="contents"
              >
                <dt className="flex min-w-0 items-center gap-1.5">
                  <AiProviderIcon
                    provider={usage.provider}
                    className="text-muted-foreground"
                    plain
                  />
                  <span className="truncate">
                    <Link
                      to="/admin/ai-billing/$userId"
                      params={{ userId: String(usage.userId) }}
                      search={{ month: overview.month }}
                      className="underline-offset-4 hover:underline"
                    >
                      {usage.developer}
                    </Link>
                    <span className="text-muted-foreground">
                      {" "}
                      · {AI_PROVIDER_LABELS[usage.provider]}
                    </span>
                  </span>
                </dt>
                <dd className="text-right font-semibold tabular-nums">
                  {formatConverted(
                    usage.totals,
                    billingCurrency,
                    overview.pendingRates,
                  )}
                </dd>
              </div>
            ))}
          </dl>
          {folded && (
            <button
              type="button"
              onClick={() => setShowSmall(true)}
              className="mt-1.5 rounded-sm text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-hidden"
            >
              Show {small.length} more under 1 {billingCurrency} each
            </button>
          )}
        </div>
      )}
    </section>
  );
}

/** Where an exponent sits across the plot, in percent. */
function tickPosition(exponent: number, axis: MultipleAxis): number {
  return ((exponent - axis.min) / (axis.max - axis.min)) * 100;
}

/** What the month's plans cost, for each provider billed at API prices that
 * has plans of its own. */
function planHints(
  overview: AiBillingOverview,
  providers: readonly AiProvider[],
): string {
  return [...new Set(providers)]
    .flatMap((provider) => {
      const range = planFeeRange(
        overview.subscriptions,
        provider,
        overview.billingCurrency,
      );
      if (range === null) {
        return [];
      }
      const low = formatAmount(range.min, overview.billingCurrency);
      const fees =
        range.min === range.max
          ? low
          : `${low} to ${formatAmount(range.max, overview.billingCurrency)}`;
      return [
        ` ${AI_PROVIDER_LABELS[provider]} plans declared this month cost ${fees} a month.`,
      ];
    })
    .join("");
}

function Stat({
  label,
  value,
  note,
}: {
  label: string;
  value: string;
  note: string;
}) {
  return (
    <div className="min-w-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="text-lg font-semibold tabular-nums">{value}</dd>
      <dd className="text-xs text-muted-foreground">{note}</dd>
    </div>
  );
}

function WarningIcon() {
  return (
    <AlertTriangle
      aria-hidden
      className="size-3.5 shrink-0 text-[#c98500] dark:text-[#fab219]"
    />
  );
}

/** A plan's worth in a word or two, beside its row. */
function worthLabel(value: SubscriptionValue, amounts: Amounts): string {
  switch (value.worth.kind) {
    case "multiple":
      return `${value.worth.atLeast ? "≥ " : ""}${formatMultiple(value.worth.multiple)}`;
    case "noUsage":
      return "No usage";
    case "noCost":
      return "0×";
    case "unpriced":
      return "Unknown";
    case "noRate":
      return amounts.ratePending ? PENDING_RATE_LABEL : "No rate";
    case "noFee":
      return "No fee";
  }
}

/** Why a plan has no bar, where its bar would be. */
function noBarText(value: SubscriptionValue, amounts: Amounts): string {
  switch (value.worth.kind) {
    case "multiple":
      return "";
    case "noUsage":
      return "No usage on the days it covers";
    case "noCost":
      return "Its usage costs nothing at API prices";
    case "unpriced":
      return "Its usage has no priced cost to compare";
    case "noRate":
      return amounts.ratePending
        ? `Fetching the rate to ${amounts.currency}`
        : `Not converted to ${amounts.currency}: no exchange rate`;
    case "noFee":
      return "A fee of nothing has no multiple";
  }
}

/** A plan's fee and what its usage would have cost, side by side. */
function sums(value: SubscriptionValue, amounts: Amounts): string {
  const { subscription } = value;
  const fee =
    value.fee === null
      ? formatFee(subscription.proratedFee, subscription.currency)
      : formatAmount(value.fee, amounts.currency);
  const usage =
    value.worth.kind === "noUsage"
      ? "nothing"
      : value.usage === null || value.worth.kind === "unpriced"
        ? formatEstimate(subscription.usage)
        : `${formatAmount(value.usage, amounts.currency)}${subscription.usage.unpricedRecords > 0 ? " + unknown" : ""}`;
  return `${fee} → ${usage}`;
}

function ValueRow({
  value,
  axis,
  amounts,
}: {
  value: SubscriptionValue;
  axis: MultipleAxis;
  amounts: Amounts;
}) {
  const { subscription } = value;
  return (
    <div className={`${ROW_GRID} border-t border-border/40 py-1.5`}>
      <div className="min-w-0 [grid-area:who]">
        <div className="truncate text-sm font-medium">{value.developer}</div>
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <AiProviderIcon
            provider={subscription.provider}
            className="text-muted-foreground"
            plain
          />
          <span className="truncate">
            {subscription.plan}
            {subscription.coveredDays < subscription.daysInMonth &&
              ` · ${subscription.coveredDays} of ${subscription.daysInMonth} days`}
          </span>
        </div>
      </div>
      <div className="min-w-0 [grid-area:plot]">
        {value.worth.kind === "multiple" ? (
          <ValueBar
            value={value}
            multiple={value.worth.multiple}
            axis={axis}
            amounts={amounts}
          />
        ) : (
          <p className="text-xs text-muted-foreground">
            {noBarText(value, amounts)}
          </p>
        )}
      </div>
      <div className="flex items-center justify-end gap-1 text-sm font-semibold tabular-nums [grid-area:worth]">
        {verdict(value.worth) === "below" && <WarningIcon />}
        {worthLabel(value, amounts)}
      </div>
      <div className="text-xs tabular-nums text-muted-foreground [grid-area:sums] sm:text-right">
        {sums(value, amounts)}
      </div>
    </div>
  );
}

/** One plan's multiple as a bar from 1×, on the axis every row shares. */
function ValueBar({
  value,
  multiple,
  axis,
  amounts,
}: {
  value: SubscriptionValue;
  multiple: number;
  axis: MultipleAxis;
  amounts: Amounts;
}) {
  const label = `${value.developer}, ${value.subscription.plan}`;
  const definition = useMemo(() => {
    const rows = [{ exponent: multipleExponent(multiple, axis) }];
    return defineChart({
      marks: [
        ruleX(
          axis.ticks.filter((tick) => tick !== 0),
          { stroke: GRID_COLOR, strokeOpacity: 1 },
        ),
        ruleX([0], { stroke: "hsl(var(--foreground))", strokeOpacity: 0.55 }),
        barX(rows, {
          x1: 0,
          x2: "exponent",
          y: () => ROW,
          fill: FILLS[verdict(value.worth)],
          maxThickness: 14,
          radius: { end: 4 },
          states: [
            {
              when: { focus: "primary" },
              style: { stroke: "hsl(var(--foreground))", strokeWidth: 1.5 },
            },
          ],
        }),
      ],
      scales: {
        x: { scale: scaleLinear().domain([axis.min, axis.max]), axis: false },
        y: { scale: scaleBand<string>().domain([ROW]), axis: false },
      },
      guides: false,
      margin: 0,
      theme: USAGE_CHART_THEME,
      focus: "nearest-x",
      maxFocusDistance: Number.POSITIVE_INFINITY,
      focusRing: false,
      tooltip: {
        ...USAGE_CHART_TOOLTIP,
        format: () =>
          `${label}\n${worthLabel(value, amounts)} the fee\n${sums(value, amounts)}`,
      },
    });
  }, [value, multiple, axis, amounts, label]);

  return (
    <Chart
      definition={definition}
      height={BAR_HEIGHT}
      className="min-w-0"
      ariaLabel={`${label}: ${worthLabel(value, amounts)} its fee`}
      ariaDescription="The bar runs from break-even, 1×, to the usage's API-equivalent cost as a multiple of the fee, on a logarithmic scale."
      renderTooltipBody={() => <ValueTooltip value={value} amounts={amounts} />}
    />
  );
}

function ValueTooltip({
  value,
  amounts,
}: {
  value: SubscriptionValue;
  amounts: Amounts;
}) {
  const { subscription } = value;
  const converted = subscription.currency !== amounts.currency;
  return (
    <div className="min-w-52 rounded-lg border border-border/60 bg-card px-3 py-2 text-sm shadow-elevated">
      <div className="font-semibold">
        {value.developer}
        <span className="font-normal text-muted-foreground">
          {" "}
          · {subscription.plan}
        </span>
      </div>
      <dl className="mt-1 text-xs">
        <dt className="text-muted-foreground">
          Fee, {subscription.coveredDays} of {subscription.daysInMonth} days
        </dt>
        <dd className="tabular-nums">
          <span className="font-semibold">
            {value.fee === null
              ? "No rate"
              : formatAmount(value.fee, amounts.currency)}
          </span>
          {converted && (
            <span className="text-muted-foreground">
              {" "}
              {formatFee(subscription.proratedFee, subscription.currency)}
            </span>
          )}
        </dd>
        <dt className="mt-1 text-muted-foreground">
          Usage it paid for, at API prices
        </dt>
        <dd className="tabular-nums">
          <span className="font-semibold">
            {value.usage === null
              ? "No rate"
              : formatAmount(value.usage, amounts.currency)}
          </span>
          {amounts.currency !== "USD" && (
            <span className="text-muted-foreground">
              {" "}
              {formatUsd(subscription.usage.apiEquivalentUsd)}
            </span>
          )}
        </dd>
      </dl>
      <div className="mt-1.5 border-t border-border/60 pt-1.5 text-xs text-muted-foreground">
        {subscription.usage.unpricedRecords > 0
          ? `${subscription.usage.unpricedRecords} unpriced records are not counted: at least ${worthLabel(value, amounts).replace("≥ ", "")}.`
          : `${worthLabel(value, amounts)} the fee · split by API-equivalent cost`}
      </div>
    </div>
  );
}
