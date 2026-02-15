import { atomWithStorage } from "jotai/utils";

export type MemberFilter = {
  mode: "mine" | "all" | "custom";
  selectedEmails: string[];
};

export type LastViewedProject = {
  organization: string;
  project: string;
} | null;

export type HiddenColumnsByScope = Record<string, string[]>;

export const memberFilterAtom = atomWithStorage<MemberFilter>(
  "board-member-filter",
  { mode: "mine", selectedEmails: [] },
);

export const lastViewedProjectAtom = atomWithStorage<LastViewedProject>(
  "board-last-viewed-project",
  null,
);

export const categoryFilterAtom = atomWithStorage<string[]>(
  "board-category-filter",
  ["userStory", "bug", "task", "feature", "epic"],
);

export const hiddenColumnsByScopeAtom = atomWithStorage<HiddenColumnsByScope>(
  "board-hidden-columns-by-scope",
  {},
);

export function boardColumnScopeKey({
  organization,
  project,
  team,
}: {
  organization: string;
  project: string;
  team?: string;
}) {
  const normalizedTeam = team?.trim() || `${project} Team`;
  return `${organization}/${project}/${normalizedTeam}`;
}
