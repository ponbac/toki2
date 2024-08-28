import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { RepoKey } from "./api/queries/queries";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function toRepoKey<
  T extends { organization: string; project: string; repoName: string },
>(obj: T): RepoKey {
  return {
    organization: obj.organization,
    project: obj.project,
    repoName: obj.repoName,
  };
}

export function toRepoKeyString<
  T extends { organization: string; project: string; repoName: string },
>(obj: T): string {
  return `${obj.organization}/${obj.project}/${obj.repoName}`;
}

export function getWeekNumber(date: Date) {
  const d = new Date(
    Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()),
  );
  const dayNum = d.getUTCDay() || 7;
  d.setUTCDate(d.getUTCDate() + 4 - dayNum);
  const yearStart = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
  return Math.ceil(((d.getTime() - yearStart.getTime()) / 86400000 + 1) / 7);
}

export type LinkData = {
  organization: string;
  project: string;
  repoName: string;
  id: number;
};

export function pullRequestUrl<T extends LinkData>(pr: T) {
  return `https://dev.azure.com/${pr.organization}/${pr.project}/_git/${pr.repoName}/pullrequest/${pr.id}`;
}

export function workItemUrl<T extends LinkData>(wi: T) {
  return `https://dev.azure.com/${wi.organization}/${wi.project}/${wi.repoName}/_workitems/edit/${wi.id}`;
}
