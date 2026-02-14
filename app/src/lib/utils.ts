import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { RepoKey } from "./api/queries/queries";
import { AvatarOverride } from "./api/queries/pullRequests";

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

export function formatHoursAsHoursMinutes(hours: number | string) {
  const hoursNum = typeof hours === "string" ? parseFloat(hours) : hours;
  const wholeHours = Math.floor(hoursNum);
  const minutes = Math.round((hoursNum - wholeHours) * 60);

  if (wholeHours > 0) {
    return `${wholeHours}h ${minutes}m`;
  } else {
    return `${minutes}m`;
  }
}

export function formatHoursMinutes(hours: number) {
  const isNegative = hours < 0;
  const absHours = Math.abs(hours);
  const hrs = Math.floor(absHours);
  const mins = Math.round((absHours - hrs) * 60);

  const formattedHrs = String(hrs).padStart(2, "0");
  const formattedMins = String(mins).padStart(2, "0");

  return `${isNegative ? "-" : ""}${formattedHrs}:${formattedMins}`;
}

export function buildAvatarOverrideMap(overrides: AvatarOverride[]) {
  return overrides.reduce<Record<string, string>>((acc, override) => {
    acc[override.email.toLowerCase()] = override.avatarUrl;
    return acc;
  }, {});
}
