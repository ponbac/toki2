/*
 * Exchange-rate input for the admin override form, checked as the server
 * checks it, so a refusal names its reason before anything is sent.
 */

/** The server's bounds for a rate: above 0, at most a million, at most six
 * decimals. */
const MAX_RATE = 1_000_000;
const MAX_RATE_DECIMALS = 6;

/**
 * A rate as an admin types it, with `.` or `,` as the decimal separator: the
 * decimal to send, or why the server would refuse it.
 */
export function parseRateInput(
  raw: string,
): { rate: string } | { error: string } {
  const text = raw.trim().replace(",", ".");
  if (text === "") {
    return { error: "Enter a rate." };
  }
  if (!/^\d+(\.\d+)?$/.test(text)) {
    return { error: "Use digits, with . or , before the decimals." };
  }
  const [units, decimals = ""] = text.split(".");
  if (decimals.length > MAX_RATE_DECIMALS) {
    return { error: `At most ${MAX_RATE_DECIMALS} decimals.` };
  }
  const value = Number(text);
  if (!(value > 0)) {
    return { error: "The rate must be above 0." };
  }
  if (value > MAX_RATE) {
    return { error: "The rate can be at most 1,000,000." };
  }
  return {
    rate: decimals ? `${Number(units)}.${decimals}` : String(Number(units)),
  };
}

/**
 * What saving or resetting a month's rate did. An open month bills at the
 * new rate; a closed month keeps the rate it was invoiced at, and the
 * difference in the billing currency posts as an adjustment on
 * `adjustmentsPostOn`, the next open month.
 */
export function rateChangeMessage({
  monthLabel,
  currency,
  rate,
  billingCurrency,
  reset,
  adjustmentsPostOn,
}: {
  /** The month the rate is for, in words. */
  monthLabel: string;
  currency: string;
  /** The rate that applies now. */
  rate: string;
  billingCurrency: string;
  /** Back at the fetched rate rather than overridden. */
  reset: boolean;
  /** A closed month: where its adjustments post, in words. */
  adjustmentsPostOn: string | null;
}): string {
  const source = reset ? "the Riksbank rate, " : "";
  if (adjustmentsPostOn === null) {
    return reset
      ? `${currency} is back at ${source}${rate} ${billingCurrency}`
      : `${monthLabel} bills ${currency} at ${rate} ${billingCurrency}`;
  }
  return `${monthLabel} keeps the rate it was invoiced at. ${currency} at ${source}${rate} ${billingCurrency} posts any ${billingCurrency} difference as an adjustment on ${adjustmentsPostOn}`;
}
