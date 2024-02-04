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
