import { atomWithStorage } from "jotai/utils";

export type MemberFilter = {
  mode: "mine" | "all" | "custom";
  selectedEmails: string[];
};

export type MemberFilterByScope = Record<string, MemberFilter>;

export type LastViewedProject = {
  organization: string;
  project: string;
} | null;

export type HiddenColumnsByScope = Record<string, string[]>;

export const DEFAULT_MEMBER_FILTER: MemberFilter = {
  mode: "mine",
  selectedEmails: [],
};

export const memberFilterByScopeAtom = atomWithStorage<MemberFilterByScope>(
  "board-member-filter-by-scope",
  {},
);

export const lastViewedProjectAtom = atomWithStorage<LastViewedProject>(
  "board-last-viewed-project",
  null,
);

export const categoryFilterAtom = atomWithStorage<string[]>(
  "board-category-filter",
  ["userStory", "bug", "task", "feature", "epic", "other"],
);

export const hiddenColumnsByScopeAtom = atomWithStorage<HiddenColumnsByScope>(
  "board-hidden-columns-by-scope",
  {},
);

function normalizeScopePart(value: string): string {
  return value.trim().toLowerCase();
}

export function boardProjectScopeKey({
  organization,
  project,
}: {
  organization: string;
  project: string;
}) {
  return JSON.stringify([
    normalizeScopePart(organization),
    normalizeScopePart(project),
  ]);
}

export function boardColumnScopeKey({
  organization,
  project,
  team,
}: {
  organization: string;
  project: string;
  team?: string;
}) {
  const normalizedTeam = normalizeScopePart(team?.trim() || `${project} Team`);
  return JSON.stringify([
    normalizeScopePart(organization),
    normalizeScopePart(project),
    normalizedTeam,
  ]);
}
