import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import type {
  AiBillingCompleteness,
  AiDeveloperCompleteness,
  AiMachineCompleteness,
  AiUploadGap,
} from "@/lib/api/queries/ai-admin";

/*
 * A billing month's upload coverage, day by day. The server has already
 * decided every gap; this lays the gaps out over the month's local days and
 * counts them. A day is uploaded when a sync covered it, whether or not it had
 * usage, and nothing here is finer than a local day.
 */

/** What a strip shows for one local day. */
export type CoverageState = "uploaded" | "missing" | "notRequired";

/** One local day of a strip, and what its state means in words. */
export type CoverageCell = Readonly<{
  day: string;
  state: CoverageState;
  text: string;
}>;

/** A provider's uploads from one machine. */
export type ProviderCoverage = AiMachineCompleteness["providers"][number];

/** How much of what a month requires has been uploaded. */
export type CoverageTotals = Readonly<{
  /** Machines active in the month: the ones that must upload. */
  machines: number;
  /** Days of the month through its last required day. */
  requiredDays: number;
  /** `machines × requiredDays`. */
  required: number;
  /** Required machine-days no provider uploaded. */
  missing: number;
  /** Uploaded machine-days on which an expected provider is missing. */
  providerOnly: number;
}>;

const NOT_YET = "Not required yet: a sync covers a day once it is over";
const INACTIVE = "Machine not active this month: nothing required";

/** Readiness that holds billing back. */
export function needsAttention(developer: AiDeveloperCompleteness): boolean {
  return (
    developer.readiness === "noSync" || developer.readiness === "incomplete"
  );
}

/** Every local day of `month` (`YYYY-MM`), `YYYY-MM-DD`. */
export function monthDays(month: string): string[] {
  const [year, monthNumber] = month.split("-").map(Number);
  const count = new Date(Date.UTC(year, monthNumber, 0)).getUTCDate();
  return Array.from(
    { length: count },
    (_, index) => `${month}-${String(index + 1).padStart(2, "0")}`,
  );
}

/** The UTC instant at midnight of a local calendar day. */
function instant(day: string): number {
  const [year, month, date] = day.split("-").map(Number);
  return Date.UTC(year, month - 1, date);
}

/** Whether a local calendar day is a Monday. */
export function isMonday(day: string): boolean {
  return new Date(instant(day)).getUTCDay() === 1;
}

/** How many days a set of inclusive, disjoint gaps spans. */
export function gapDayCount(gaps: readonly AiUploadGap[]): number {
  const dayMs = 24 * 60 * 60 * 1000;
  return gaps.reduce(
    (count, gap) =>
      count + Math.round((instant(gap.to) - instant(gap.from)) / dayMs) + 1,
    0,
  );
}

/** Whether `day` is one a machine must have uploaded by now. */
function isRequired(day: string, requiredThrough: string | null): boolean {
  return requiredThrough !== null && day <= requiredThrough;
}

function inGaps(gaps: readonly AiUploadGap[], day: string): boolean {
  return gaps.some((gap) => gap.from <= day && day <= gap.to);
}

function sameGaps(
  a: readonly AiUploadGap[],
  b: readonly AiUploadGap[],
): boolean {
  return (
    a.length === b.length &&
    a.every(
      (gap, index) => gap.from === b[index].from && gap.to === b[index].to,
    )
  );
}

/**
 * The providers whose missing days are not simply the machine's: the ones
 * that need a strip of their own.
 */
export function providersWithOwnGaps(
  machine: AiMachineCompleteness,
): ProviderCoverage[] {
  return machine.providers.filter(
    (provider) =>
      provider.gaps.length > 0 && !sameGaps(provider.gaps, machine.gaps),
  );
}

/**
 * A machine's strip over `days`. An inactive machine and days after
 * `requiredThrough` are not required, never missing.
 */
export function machineCells(
  machine: AiMachineCompleteness,
  days: readonly string[],
  requiredThrough: string | null,
): CoverageCell[] {
  return days.map((day) => {
    if (!machine.activeInMonth) {
      return { day, state: "notRequired", text: INACTIVE };
    }
    if (!isRequired(day, requiredThrough)) {
      return { day, state: "notRequired", text: NOT_YET };
    }
    if (inGaps(machine.gaps, day)) {
      return { day, state: "missing", text: "Nothing uploaded" };
    }
    const without = machine.providers
      .filter((provider) => inGaps(provider.gaps, day))
      .map((provider) => AI_PROVIDER_LABELS[provider.provider]);
    return {
      day,
      state: "uploaded",
      text:
        without.length > 0
          ? `Uploaded, except ${without.join(" and ")}`
          : "Uploaded",
    };
  });
}

/** One provider's strip on an active machine, over `days`. */
export function providerCells(
  machine: AiMachineCompleteness,
  provider: ProviderCoverage,
  days: readonly string[],
  requiredThrough: string | null,
): CoverageCell[] {
  const label = AI_PROVIDER_LABELS[provider.provider];
  return days.map((day) => {
    if (!isRequired(day, requiredThrough)) {
      return { day, state: "notRequired", text: NOT_YET };
    }
    return inGaps(provider.gaps, day) || inGaps(machine.gaps, day)
      ? { day, state: "missing", text: `${label} not uploaded` }
      : { day, state: "uploaded", text: `${label} uploaded` };
  });
}

/** What the month requires of the machines active in it, and what is missing. */
export function coverageTotals(
  completeness: AiBillingCompleteness,
  days: readonly string[],
): CoverageTotals {
  const required = days.filter((day) =>
    isRequired(day, completeness.requiredThrough),
  );
  const machines = completeness.developers
    .flatMap((developer) => developer.machines)
    .filter((machine) => machine.activeInMonth);
  let missing = 0;
  let providerOnly = 0;
  for (const machine of machines) {
    for (const day of required) {
      if (inGaps(machine.gaps, day)) {
        missing += 1;
      } else if (
        machine.providers.some((provider) => inGaps(provider.gaps, day))
      ) {
        providerOnly += 1;
      }
    }
  }
  return {
    machines: machines.length,
    requiredDays: required.length,
    required: machines.length * required.length,
    missing,
    providerOnly,
  };
}
