import type { AiMonthCloseStatus } from "./contracts/ai-month-close";

/*
 * When the admin AI bill's reads are reloaded, and how a closed month's
 * frozen rate fetches read. Kept apart from the queries so they can be tested
 * without the HTTP client.
 */

/** How often a read that waits for an exchange rate is reloaded. */
export const PENDING_RATES_POLL_MS = 3_000;

/** A bill's rates the server is still fetching: its own month's, and those
 * of the closed months whose adjustment lines post on it. */
type PendingFetches = {
  pendingRates: readonly string[];
  adjustedPendingRates: readonly unknown[];
};

/** Reloads an open month's bill until the server has fetched its pending
 * rates, the corrected months' too, whose arrival may change its adjustment
 * lines (a closed month's have none, `withoutFrozenFetches`). */
export function whileRatesArePending(query: {
  state: { data?: PendingFetches };
}): number | false {
  const data = query.state.data;
  return (data?.pendingRates.length ?? 0) > 0 ||
    (data?.adjustedPendingRates.length ?? 0) > 0
    ? PENDING_RATES_POLL_MS
    : false;
}

/**
 * A closed month's bill is its snapshot: a rate still being fetched when it
 * closed never arrives in it. Its amounts stay as they were invoiced:
 * converted at the rate being refreshed, or unconverted, in `missingRates`.
 * So a closed month has no pending rates, its own or its adjustments', on
 * every view of it.
 */
export function withoutFrozenFetches<
  T extends PendingFetches & { close: unknown },
>(bill: T): T {
  return bill.close === null ||
    (bill.pendingRates.length === 0 && bill.adjustedPendingRates.length === 0)
    ? bill
    : { ...bill, pendingRates: [], adjustedPendingRates: [] };
}

/** Whether a month's readiness waits for a rate the server is still
 * fetching, its own or a corrected month's, which its next evaluation may
 * have. */
export function readinessAwaitsRates(
  status: AiMonthCloseStatus | undefined,
): boolean {
  return (status?.readiness?.checks ?? []).some((check) =>
    check.problems.some(
      (problem) =>
        problem.kind === "ratePending" ||
        problem.kind === "adjustedRatePending",
    ),
  );
}

/** Reloads a month's close status while its readiness awaits a rate. */
export function whileReadinessAwaitsRates(query: {
  state: { data?: AiMonthCloseStatus };
}): number | false {
  return readinessAwaitsRates(query.state.data) ? PENDING_RATES_POLL_MS : false;
}
