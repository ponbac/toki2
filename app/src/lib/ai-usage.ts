/**
 * The name of AI usage a developer marked as not client work: its repository
 * is their own or internal, and its usage is left out of the bill.
 */
export const PERSONAL_LABEL = "Personal / internal";

type CoverageStatus = "ok" | "partial" | "failed" | "missing";

/** A tool's last read as Toki shows it: never `partial`. */
export type ShownCoverageStatus = Exclude<CoverageStatus, "partial">;

/**
 * How a tool's last read is shown. Token Ledger reports `partial` only for
 * single malformed lines or records it skipped, worth cents of usage at most;
 * a file it could not read makes the read `failed`. So `partial` is shown as
 * `ok`, and the skipped counts stay in the database for anyone who asks.
 */
export function shownCoverageStatus(
  status: CoverageStatus,
): ShownCoverageStatus {
  return status === "partial" ? "ok" : status;
}
