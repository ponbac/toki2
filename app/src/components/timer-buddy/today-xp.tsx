import dayjs from "dayjs";
import type { BuddyToday } from "@/lib/api/queries/buddy";
import { formatBuddyXp, formatLoggedHours } from "@/lib/buddy/format";
import { BATTLE_XP, HOURS_XP_PER_HOUR, MAX_XP_HOURS } from "@/lib/buddy/level";
import { cn } from "@/lib/utils";

/**
 * The XP the user's buddies got today and the rules that give it. `syncedAt`
 * is when the logged hours were last read; `twoBuddies` says whether a
 * partner shares the XP. `className` styles the card.
 */
export function TodayXp({
  today,
  syncedAt,
  twoBuddies,
  className,
}: {
  today: BuddyToday;
  syncedAt: string | null;
  twoBuddies: boolean;
  className?: string;
}) {
  return (
    <div className={cn("border border-border/60 bg-muted/30", className)}>
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-sm font-semibold">Today</h3>
        {syncedAt && (
          <p className="text-xs text-muted-foreground">
            Hours as of {formatSyncedAt(syncedAt)}
          </p>
        )}
      </div>
      <dl className="mt-2 grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-1.5 text-sm">
        <dt className="text-muted-foreground">
          <span className="font-semibold text-foreground">
            {formatLoggedHours(today.loggedHours)}
          </span>{" "}
          logged
        </dt>
        <dd className="text-right font-mono text-xs font-semibold tabular-nums leading-5">
          +{formatBuddyXp(today.hoursXp)}
        </dd>
        <dt className="text-muted-foreground">
          <span className="font-semibold text-foreground">
            {today.battles.toLocaleString("en-US")}{" "}
            {today.battles === 1 ? "battle" : "battles"}
          </span>{" "}
          started
        </dt>
        <dd className="text-right font-mono text-xs font-semibold tabular-nums leading-5">
          +{formatBuddyXp(today.battleXp)}
        </dd>
      </dl>
      <p className="mt-3 text-xs text-muted-foreground">
        {HOURS_XP_PER_HOUR} XP an hour, up to {MAX_XP_HOURS} hours a day,
        weekends too, and {BATTLE_XP} XP for each battle you start.
        {twoBuddies && " Both buddies get all of it."}
      </p>
    </div>
  );
}

/** The time of the last read, with its date when that was not today. */
function formatSyncedAt(syncedAt: string): string {
  const read = dayjs(syncedAt);
  return read.format(read.isSame(dayjs(), "day") ? "HH:mm" : "D MMM, HH:mm");
}
