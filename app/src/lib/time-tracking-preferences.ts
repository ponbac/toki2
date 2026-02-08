import { atomWithStorage } from "jotai/utils";

export type LastProject = {
  projectId: string;
  projectName: string;
} | null;

export type LastActivity = {
  activityId: string;
  activityName: string;
} | null;

export const lastProjectAtom = atomWithStorage<LastProject>(
  "time-tracking-lastProject",
  null,
);

export const lastActivityAtom = atomWithStorage<LastActivity>(
  "time-tracking-lastActivity",
  null,
);

export const rememberLastProjectAtom = atomWithStorage(
  "time-tracking-rememberLastProject",
  false,
);

/**
 * Builds the project/activity params for starting a timer based on remembered preferences.
 */
export function buildRememberedTimerParams(opts: {
  rememberLastProject: boolean;
  lastProject: LastProject;
  lastActivity: LastActivity;
}): {
  projectId?: string;
  projectName?: string;
  activityId?: string;
  activityName?: string;
} {
  const { rememberLastProject, lastProject, lastActivity } = opts;

  if (!rememberLastProject) {
    return {};
  }

  return {
    ...(lastProject
      ? {
          projectId: lastProject.projectId,
          projectName: lastProject.projectName,
        }
      : {}),
    ...(lastActivity
      ? {
          activityId: lastActivity.activityId,
          activityName: lastActivity.activityName,
        }
      : {}),
  };
}
