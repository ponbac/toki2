import type {
  AiRepositoryMappingRow,
  AiRepositoryMappingStatus,
  AiRepositoryMonthCredits,
  AiRepositorySuggestion,
} from "@/lib/api/queries/ai-admin";
import type { AiUsageProject } from "@/lib/api/queries/ai-usage-report";
import { creditHundredths } from "./reconciliation";

/*
 * Pure helpers for the GitHub repositories on Project mappings: where each
 * repository's code review credits bill, and which ones need a look.
 */

/** Where a repository's credits bill: through its mapping's status, or
 * `unmapped` without one. Only `resolves` bills a project; every other state
 * bills Unassigned. */
export type RepositoryStatus = AiRepositoryMappingStatus | "unmapped";

/** Where a repository's credits bill. */
export function repositoryStatus(
  row: AiRepositoryMappingRow,
): RepositoryStatus {
  return row.mapping?.status ?? "unmapped";
}

/** Whether a repository needs a look: a shown month whose credits bill a
 * line (`billed`, as the server rounds them) as Unassigned, which will hold
 * back month close. Credits before the cutover or under one credit for the
 * month bill no line, so they never do. */
export function repositoryNeedsLook(row: AiRepositoryMappingRow): boolean {
  return (
    repositoryStatus(row) !== "resolves" &&
    row.months.some(({ billed }) => billed)
  );
}

/** Why a month's credits bill no line, as their cell's title: no cutover,
 * a month before it, or under one credit for the month. `null` when they
 * bill a line or there are none. */
export function unbilledCreditsReason(
  { month, credits, billed }: AiRepositoryMonthCredits,
  billedFrom: string | null,
): string | null {
  if (billed || creditHundredths(credits) === 0) {
    return null;
  }
  if (billedFrom === null) {
    return "Not billed: no cutover month is configured";
  }
  return month < billedFrom
    ? "Not billed: before the cutover month"
    : "Not billed: under one credit for the month";
}

/** The mapped project keys behind a suggestion, as its button's title. */
export function suggestionTitle(suggestion: AiRepositorySuggestion): string {
  const [first, ...rest] = suggestion.projectKeys;
  return rest.length === 0
    ? `Mapped key: ${first}`
    : `Mapped keys: ${suggestion.projectKeys.join(", ")}`;
}

/** The suggestions an admin can take: those of an active project, as the
 * picker lists. A project-key mapping keeps its project once the project is
 * archived, yet a repository maps to active projects only. None until the
 * active projects are known. */
export function activeSuggestions(
  suggestions: readonly AiRepositorySuggestion[],
  activeProjects: readonly AiUsageProject[] | undefined,
): AiRepositorySuggestion[] {
  if (activeProjects === undefined) {
    return [];
  }
  const active = new Set(activeProjects.map((project) => project.projectId));
  return suggestions.filter((suggestion) =>
    active.has(suggestion.project.projectId),
  );
}
