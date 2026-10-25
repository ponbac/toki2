import type {
  AiBillingDeveloper,
  AiBillingLine,
  AiExchangeRate,
  AiSubscriptionMonth,
} from "@/lib/api/queries/ai-admin";
import type { AiProvider } from "@/lib/api/queries/ai-usage";
import { billableTotals, lineUserId, type BillableTotals } from "./billing";

/*
 * What each subscription's fee bought: the fee beside what the usage it paid
 * for would have cost at API prices, as a multiple of the fee. Both are put in
 * the billing currency to be compared; these are estimates for reading, and
 * nothing here changes what a bill charges.
 */

/** What a subscription's fee bought, as far as it can be said. */
export type SubscriptionWorth =
  /** The usage's API-equivalent cost over the fee; a floor when some usage
   * is unpriced. */
  | Readonly<{ kind: "multiple"; multiple: number; atLeast: boolean }>
  /** No usage on the days the subscription covers: the fee is overhead. */
  | Readonly<{ kind: "noUsage" }>
  /** Usage that is all priced, and costs nothing at API prices. */
  | Readonly<{ kind: "noCost" }>
  /** Usage whose only known cost is nothing, with unpriced records beside. */
  | Readonly<{ kind: "unpriced" }>
  /** The fee or the estimate cannot be put in the billing currency. */
  | Readonly<{ kind: "noRate" }>
  /** A fee of nothing has no multiple. */
  | Readonly<{ kind: "noFee" }>;

/** One subscription's month, with its fee and usage in the billing currency. */
export type SubscriptionValue = Readonly<{
  subscription: AiSubscriptionMonth;
  developer: string;
  /** The pro-rated fee; null without a rate for its currency. */
  fee: number | null;
  /** The API-equivalent cost of the priced usage; null when there is some
   * but no USD rate for it. */
  usage: number | null;
  worth: SubscriptionWorth;
}>;

/**
 * Whether a plan's usage was worth its fee. It is `open` where that cannot be
 * told: nothing to compare, or a floor below the fee that unpriced usage may
 * yet lift above it.
 */
export type SubscriptionVerdict = "above" | "below" | "open";

/** A month's subscriptions together. */
export type SubscriptionValueSummary = Readonly<{
  plans: number;
  /** Plans whose fee and usage are both known in the billing currency. */
  measured: number;
  /** Measured plans with a verdict other than `open`. */
  decided: number;
  /** Decided plans whose usage was worth less than the fee. */
  below: number;
  /** The measured plans' fees and usage, and the one over the other. */
  fees: number;
  usage: number;
  overall: number | null;
  /** Some measured usage is unpriced, so `usage` and `overall` are floors. */
  atLeast: boolean;
}>;

/**
 * Units of the billing currency per US dollar as the month's bill has it, or
 * null when the bill has no dollar amounts and so no dollar rate.
 */
export function billUsdRate(
  rates: readonly AiExchangeRate[],
  billingCurrency: string,
): number | null {
  if (billingCurrency === "USD") {
    return 1;
  }
  const usd = rates.find((rate) => rate.currency === "USD");
  return usd ? Number(usd.rate) : null;
}

function worth(
  subscription: AiSubscriptionMonth,
  fee: number | null,
  usdRate: number | null,
): SubscriptionWorth {
  const usd = subscription.usage.apiEquivalentUsd;
  if (fee === 0) {
    return { kind: "noFee" };
  }
  if (subscription.allocation === "unallocated") {
    return { kind: "noUsage" };
  }
  if (usd === 0) {
    return subscription.usage.unpricedRecords > 0
      ? { kind: "unpriced" }
      : { kind: "noCost" };
  }
  if (fee === null || usdRate === null) {
    return { kind: "noRate" };
  }
  return {
    kind: "multiple",
    multiple: (usd * usdRate) / fee,
    atLeast: subscription.usage.unpricedRecords > 0,
  };
}

/** Whether a plan's usage was worth its fee, where that can be told. */
export function verdict(worth: SubscriptionWorth): SubscriptionVerdict {
  switch (worth.kind) {
    case "multiple":
      if (worth.multiple >= 1) {
        return "above";
      }
      return worth.atLeast ? "open" : "below";
    case "noUsage":
    case "noCost":
      return "below";
    case "unpriced":
    case "noRate":
    case "noFee":
      return "open";
  }
}

/** Where a plan sorts: the worst first, the ones that cannot be judged last. */
function sortKey(value: SubscriptionValue): number {
  switch (value.worth.kind) {
    case "noUsage":
    case "noCost":
      return 0;
    case "multiple":
      return value.worth.multiple;
    case "unpriced":
    case "noRate":
    case "noFee":
      return Number.POSITIVE_INFINITY;
  }
}

/**
 * Each subscription's fee and usage in the billing currency, the plans that
 * bought least first. `usdRate` is units of the billing currency per US
 * dollar, or null when unknown.
 */
export function subscriptionValues(
  subscriptions: readonly AiSubscriptionMonth[],
  developers: readonly AiBillingDeveloper[],
  usdRate: number | null,
): SubscriptionValue[] {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  return subscriptions
    .map((subscription): SubscriptionValue => {
      const fee =
        subscription.convertedProratedFee === null
          ? null
          : Number(subscription.convertedProratedFee);
      return {
        subscription,
        developer:
          names.get(subscription.userId) ?? `User ${subscription.userId}`,
        fee,
        // Nothing priced is nothing in any currency, with or without a rate.
        usage:
          subscription.usage.apiEquivalentUsd === 0
            ? 0
            : usdRate === null
              ? null
              : subscription.usage.apiEquivalentUsd * usdRate,
        worth: worth(subscription, fee, usdRate),
      };
    })
    .sort(
      (a, b) =>
        sortKey(a) - sortKey(b) ||
        a.developer.localeCompare(b.developer) ||
        a.subscription.subscriptionId - b.subscription.subscriptionId,
    );
}

/** The plans whose fee and usage are both known, summed. */
export function summarizeValues(
  values: readonly SubscriptionValue[],
): SubscriptionValueSummary {
  let measured = 0;
  let decided = 0;
  let below = 0;
  let fees = 0;
  let usage = 0;
  let atLeast = false;
  for (const value of values) {
    if (value.fee === null || value.usage === null) {
      continue;
    }
    switch (value.worth.kind) {
      case "multiple":
        atLeast ||= value.worth.atLeast;
        break;
      case "noUsage":
      case "noCost":
        break;
      case "unpriced":
      case "noRate":
      case "noFee":
        continue;
    }
    const outcome = verdict(value.worth);
    measured += 1;
    decided += outcome === "open" ? 0 : 1;
    below += outcome === "below" ? 1 : 0;
    fees += value.fee;
    usage += value.usage;
  }
  return {
    plans: values.length,
    measured,
    decided,
    below,
    fees,
    usage,
    overall: fees > 0 ? usage / fees : null,
    atLeast,
  };
}

/** The log₂ extent a set of multiples is drawn on, and its ticks. */
export type MultipleAxis = Readonly<{
  /** Exponents of two: `-2` is ¼×, `0` is 1×, `3` is 8×. */
  min: number;
  max: number;
  ticks: readonly number[];
}>;

/**
 * An axis of doublings around 1× that holds every multiple, never narrower
 * than ¼× to 4× nor wider than 1/64× to 1024×; a multiple beyond that is
 * drawn at the edge.
 */
export function multipleAxis(
  values: readonly SubscriptionValue[],
): MultipleAxis {
  const logs = values.flatMap((value) =>
    value.worth.kind === "multiple" ? [Math.log2(value.worth.multiple)] : [],
  );
  const min = Math.max(-6, Math.min(-2, Math.floor(Math.min(...logs, 0))));
  const max = Math.min(10, Math.max(2, Math.ceil(Math.max(...logs, 0))));
  const step = max - min > 8 ? 2 : 1;
  const ticks: number[] = [];
  for (let exponent = min; exponent <= max; exponent++) {
    if (exponent % step === 0) {
      ticks.push(exponent);
    }
  }
  return { min, max, ticks };
}

/** Where a multiple sits on `axis`, clamped to its ends. */
export function multipleExponent(multiple: number, axis: MultipleAxis): number {
  return Math.min(axis.max, Math.max(axis.min, Math.log2(multiple)));
}

/**
 * A multiple of the fee, such as `0.5×`, `7.1×` or `24×`. A positive multiple
 * too small for two decimals reads `<0.01×`, never as nothing.
 */
export function formatMultiple(multiple: number): string {
  if (multiple > 0 && multiple < 0.005) {
    return "<0.01×";
  }
  const digits = multiple >= 10 ? 0 : multiple >= 0.1 ? 1 : 2;
  return `${multiple.toFixed(digits)}×`;
}

/** An axis tick: `¼×`, `1×`, `8×`. */
export function formatMultipleTick(exponent: number): string {
  switch (exponent) {
    case -1:
      return "½×";
    case -2:
      return "¼×";
    case -3:
      return "⅛×";
    default:
      return exponent >= 0 ? `${2 ** exponent}×` : `1/${2 ** -exponent}×`;
  }
}

/** One developer's usage of one provider that no subscription paid for. */
export type ApiBilledUsage = Readonly<{
  userId: number;
  developer: string;
  provider: AiProvider;
  totals: BillableTotals;
}>;

/** What bills at API prices, per developer and provider, largest first. */
export function apiBilledUsage(
  lines: readonly AiBillingLine[],
  developers: readonly AiBillingDeveloper[],
): ApiBilledUsage[] {
  const names = new Map(developers.map((d) => [d.userId, d.fullName]));
  const groups = new Map<
    string,
    { userId: number; provider: AiProvider; lines: AiBillingLine[] }
  >();
  for (const line of lines) {
    const userId = lineUserId(line);
    if (line.billingMode !== "api" || userId === null) {
      continue;
    }
    const key = `${userId}:${line.provider}`;
    const group = groups.get(key) ?? {
      userId,
      provider: line.provider,
      lines: [],
    };
    group.lines.push(line);
    groups.set(key, group);
  }
  return [...groups.values()]
    .map((group): ApiBilledUsage => ({
      userId: group.userId,
      developer: names.get(group.userId) ?? `User ${group.userId}`,
      provider: group.provider,
      totals: billableTotals(group.lines),
    }))
    .sort(
      (a, b) =>
        (b.totals.converted ?? -1) - (a.totals.converted ?? -1) ||
        a.developer.localeCompare(b.developer),
    );
}

/**
 * The lowest and highest monthly fee, in the billing currency, of the month's
 * subscriptions declared for `provider`; null when none can be converted. A
 * provider without subscriptions of its own, such as Pi, has none.
 */
export function planFeeRange(
  subscriptions: readonly AiSubscriptionMonth[],
  provider: AiProvider,
  billingCurrency: string,
): Readonly<{ min: number; max: number }> | null {
  const fees = subscriptions.flatMap((subscription) => {
    if (subscription.provider !== provider) {
      return [];
    }
    const rate =
      subscription.currency === billingCurrency
        ? 1
        : subscription.exchangeRate === null
          ? null
          : Number(subscription.exchangeRate);
    return rate === null ? [] : [Number(subscription.monthlyCost) * rate];
  });
  return fees.length > 0
    ? { min: Math.min(...fees), max: Math.max(...fees) }
    : null;
}
