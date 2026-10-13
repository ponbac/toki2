/**
 * The page's tabs, one per job: reading the report, looking up a session,
 * mapping repositories, and checking the machines that sync.
 */
export const USAGE_TABS = [
  "overview",
  "sessions",
  "repositories",
  "machines",
] as const;

export type UsageTab = (typeof USAGE_TABS)[number];

/** The tab shown when the URL names none. */
export const DEFAULT_TAB: UsageTab = "overview";
