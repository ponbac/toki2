import { atomWithStorage } from "jotai/utils";

export type MemberFilter = {
  mode: "mine" | "all" | "custom";
  selectedEmails: string[];
};

export type LastViewedProject = {
  organization: string;
  project: string;
} | null;

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
