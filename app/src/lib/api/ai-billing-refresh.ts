import type { AiMonthCloseStatus } from "./contracts/ai-month-close";

/*
 * When the admin AI bill's reads are reloaded, and how a closed month's
 * frozen rate fetches read. Kept apart from the queries so they can be tested
 * without the HTTP client.
 */

/** How often a read that waits for an exchange rate is reloaded. */
export const PENDING_RATES_POLL_MS = 3_000;

/** Reloads an open month's bill until the server has fetched its pending
 * rates (a closed month's have none, `withoutFrozenFetches`). */
export function whileRatesArePending(query: {
  state: { data?: { pendingRates: readonly string[] } };
}): number | false {
  return (query.state.data?.pendingRates.length ?? 0) > 0
    ? PENDING_RATES_POLL_MS
    : false;
}

/**
 * A closed month's bill is its snapshot: a rate still being fetched when it
 * closed never arrives in it. Its amounts stay as they were invoiced:
 * converted at the rate being refreshed, or unconverted, in `missingRates`.
 * So a closed month has no pending rates, on every view of it.
 */
export function withoutFrozenFetches<
  T extends { close: unknown; pendingRates: readonly string[] },
>(bill: T): T {
  return bill.close === null || bill.pendingRates.length === 0
    ? bill
    : { ...bill, pendingRates: [] };
}

/** Whether a month's readiness waits for a rate the server is still
 * fetching, which its next evaluation may have. */
export function readinessAwaitsRates(
  status: AiMonthCloseStatus | undefined,
): boolean {
  return (status?.readiness?.checks ?? []).some((check) =>
    check.problems.some((problem) => problem.kind === "ratePending"),
  );
}

/** Reloads a month's close status while its readiness awaits a rate. */
export function whileReadinessAwaitsRates(query: {
  state: { data?: AiMonthCloseStatus };
}): number | false {
  return readinessAwaitsRates(query.state.data) ? PENDING_RATES_POLL_MS : false;
}
