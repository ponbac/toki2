import type {
  AiBillingCompleteness,
  AiDeveloperCompleteness,
  AiUploadGap,
} from "@/lib/api/queries/ai-admin";
import { monthDays } from "./coverage";

/*
 * A developer's uploads for a month, read the way an admin closing the month
 * needs them: one strip for all their machines, and their missing days told
 * apart by what they most likely mean. Missing days at the start of the month
 * are days before their first upload, such as before they set Token Ledger
 * up; the last required day of a month in progress is usually only waiting
 * for its next sync; anything else is a hole. The server decides which days
 * are missing; this only merges and names them.
 */

/** What a developer's strip shows for one local day. */
export type UploadDay =
  /** Every active machine uploaded it, from every expected tool. */
  | "uploaded"
  /** Every active machine uploaded it, but an expected tool did not. */
  | "partial"
  /** Some active machine uploaded nothing for it. */
  | "missing"
  /** Not required: after the last required day, or no machine was active. */
  | "notRequired";

/** What a run of missing days most likely means. */
export type MissingRunKind =
  /** It starts on the first of the month, before anything was uploaded. */
  | "beforeFirstUpload"
  /** A few days ending on the last required day of a month in progress. */
  | "awaitingSync"
  /** Anything else: days that should have been uploaded and were not. */
  | "hole";

/** Inclusive local days missing in a row. */
export type MissingRun = Readonly<{
  from: string;
  to: string;
  days: number;
  kind: MissingRunKind;
}>;

/** How far billing can rely on a developer's uploads, worst first. */
export type UploadVerdict =
  /** No machine has ever synced, though usage or a plan is on record. */
  | "neverSynced"
  /** Days in the middle of the month are missing. */
  | "holes"
  /** Active machines, but not one required day uploaded. */
  | "nothingUploaded"
  /** An expected tool is missing on some uploaded days, whether or not
   * days before the first upload are missing too. */
  | "toolGaps"
  /** Only days before their first upload are missing, from every tool. */
  | "lateStart"
  /** Only the latest required day waits for a sync. */
  | "awaitingSync"
  /** Everything required is uploaded. */
  | "complete"
  /** None of their machines was active in the month. */
  | "noActivity";

/** Verdicts in the order they are worked through. */
export const UPLOAD_VERDICTS: readonly UploadVerdict[] = [
  "neverSynced",
  "holes",
  "nothingUploaded",
  "toolGaps",
  "lateStart",
  "awaitingSync",
  "complete",
  "noActivity",
];

/** How each verdict is shown: a badge, and the heading of its group. */
export const VERDICT_DISPLAY = {
  neverSynced: {
    tone: "critical",
    label: "Never synced",
    heading: "No machine has synced",
  },
  holes: {
    tone: "critical",
    label: "Missing days",
    heading: "Days missing mid-month",
  },
  nothingUploaded: {
    tone: "critical",
    label: "Nothing uploaded",
    heading: "Nothing uploaded this month",
  },
  lateStart: {
    tone: "neutral",
    label: "Started mid-month",
    heading: "Nothing before their first upload",
  },
  toolGaps: {
    tone: "warning",
    label: "Tool missing",
    heading: "A tool missing on some days",
  },
  awaitingSync: {
    tone: "neutral",
    label: "Awaiting sync",
    heading: "Waiting for the next sync",
  },
  complete: { tone: "good", label: "Complete", heading: "Every day uploaded" },
  noActivity: {
    tone: "neutral",
    label: "No activity",
    heading: "No machine active this month",
  },
} as const satisfies Record<
  UploadVerdict,
  {
    tone: "good" | "warning" | "critical" | "neutral";
    label: string;
    heading: string;
  }
>;

/** One developer's month of uploads. */
export type DeveloperUploads = Readonly<{
  developer: AiDeveloperCompleteness;
  /** One state per day of the month. */
  days: readonly UploadDay[];
  runs: readonly MissingRun[];
  /** Required days on which a machine uploaded but a tool did not. */
  toolDays: number;
  staleMachines: number;
  verdict: UploadVerdict;
}>;

function inGaps(gaps: readonly AiUploadGap[], day: string): boolean {
  return gaps.some((gap) => gap.from <= day && day <= gap.to);
}

/** The longest run at the end of a month in progress that is read as
 * waiting for a sync: a weekend without one, and the day after. */
const AWAITING_SYNC_DAYS = 3;

function runKind(
  run: Readonly<{ from: string; to: string; days: number }>,
  firstDay: string,
  requiredThrough: string | null,
  inProgress: boolean,
): MissingRunKind {
  if (
    inProgress &&
    run.to === requiredThrough &&
    run.days <= AWAITING_SYNC_DAYS
  ) {
    return "awaitingSync";
  }
  // A month with nothing uploaded at all is a hole, not a late start.
  if (run.from === firstDay && run.to !== requiredThrough) {
    return "beforeFirstUpload";
  }
  return "hole";
}

/** Reads one developer's machines over the month. */
export function developerUploads(
  developer: AiDeveloperCompleteness,
  completeness: Pick<
    AiBillingCompleteness,
    "month" | "requiredThrough" | "inProgress"
  >,
): DeveloperUploads {
  const { requiredThrough, inProgress } = completeness;
  const monthDayList = monthDays(completeness.month);
  const active = developer.machines.filter((machine) => machine.activeInMonth);
  const days = monthDayList.map((day): UploadDay => {
    if (
      active.length === 0 ||
      requiredThrough === null ||
      day > requiredThrough
    ) {
      return "notRequired";
    }
    if (active.some((machine) => inGaps(machine.gaps, day))) {
      return "missing";
    }
    return active.some((machine) =>
      machine.providers.some((provider) => inGaps(provider.gaps, day)),
    )
      ? "partial"
      : "uploaded";
  });

  const runs: MissingRun[] = [];
  days.forEach((state, index) => {
    if (state !== "missing") {
      return;
    }
    const day = monthDayList[index];
    const last = runs.at(-1);
    if (last && days[index - 1] === "missing") {
      runs[runs.length - 1] = { ...last, to: day, days: last.days + 1 };
    } else {
      runs.push({ from: day, to: day, days: 1, kind: "hole" });
    }
  });
  const classified = runs.map((run) => ({
    ...run,
    kind: runKind(run, monthDayList[0], requiredThrough, inProgress),
  }));

  const toolDays = days.filter((state) => state === "partial").length;
  const has = (kind: MissingRunKind) =>
    classified.some((run) => run.kind === kind);
  const verdict: UploadVerdict =
    developer.machines.length === 0
      ? developer.hasUsage || developer.hasSubscription
        ? "neverSynced"
        : "noActivity"
      : active.length === 0
        ? "noActivity"
        : days.every(
              (state) => state === "missing" || state === "notRequired",
            ) && days.includes("missing")
          ? "nothingUploaded"
          : has("hole")
            ? "holes"
            : // A missing tool may be usage that never reached the bill,
              // so it outranks a late start, which does not hold it back.
              toolDays > 0
              ? "toolGaps"
              : has("beforeFirstUpload")
                ? "lateStart"
                : has("awaitingSync")
                  ? "awaitingSync"
                  : "complete";

  return {
    developer,
    days,
    runs: classified,
    toolDays,
    staleMachines: developer.machines.filter((machine) => machine.stale).length,
    verdict,
  };
}

/** Everyone's uploads for the month, worst verdict first, then by name. */
export function monthUploads(
  completeness: AiBillingCompleteness,
): DeveloperUploads[] {
  return completeness.developers
    .map((developer) => developerUploads(developer, completeness))
    .sort(
      (a, b) =>
        UPLOAD_VERDICTS.indexOf(a.verdict) -
          UPLOAD_VERDICTS.indexOf(b.verdict) ||
        a.developer.fullName.localeCompare(b.developer.fullName),
    );
}

/** Whether a verdict holds the month's bill back. A missing tool may be
 * usage that never reached the bill, so it does. A late start does not: days
 * before someone set Token Ledger up are how every developer begins, and
 * nothing an admin does fills them. */
export function holdsBillBack(verdict: UploadVerdict): boolean {
  switch (verdict) {
    case "neverSynced":
    case "holes":
    case "nothingUploaded":
    case "toolGaps":
      return true;
    case "lateStart":
    case "awaitingSync":
    case "complete":
    case "noActivity":
      return false;
  }
}

/** How many developers have each verdict. */
export function verdictCounts(
  uploads: readonly DeveloperUploads[],
): Readonly<Record<UploadVerdict, number>> {
  const counts: Record<UploadVerdict, number> = {
    neverSynced: 0,
    holes: 0,
    nothingUploaded: 0,
    lateStart: 0,
    toolGaps: 0,
    awaitingSync: 0,
    complete: 0,
    noActivity: 0,
  };
  for (const upload of uploads) {
    counts[upload.verdict] += 1;
  }
  return counts;
}
