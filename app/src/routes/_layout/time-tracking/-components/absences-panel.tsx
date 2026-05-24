import React from "react";
import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import { CalendarX2, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import type {
  AbsenceEntry,
  DateRangeQuery,
} from "@/lib/api/queries/time-tracking";
import { cn } from "@/lib/utils";
import { absenceTypeIcons, absenceTypeAccent } from "./absence-types";
import { AbsenceDeleteDialog } from "./absence-delete-dialog";

type AbsenceGroup = {
  date: string;
  entries: Array<AbsenceEntry>;
  hours: number;
};

const EMPTY_ABSENCES: Array<AbsenceEntry> = [];

export function AbsencesPanel({ dateRange }: { dateRange: DateRangeQuery }) {
  const [pendingDelete, setPendingDelete] = React.useState<AbsenceEntry | null>(
    null,
  );
  const { data: absences = EMPTY_ABSENCES, isLoading } = useQuery(
    timeTrackingQueries.absenceEntries(dateRange),
  );

  const { groups, totalHours } = React.useMemo(() => {
    const byDate = new Map<string, AbsenceGroup>();
    let hours = 0;

    for (const entry of absences) {
      hours += entry.hours;
      const group = byDate.get(entry.date);
      if (group) {
        group.entries.push(entry);
        group.hours += entry.hours;
        continue;
      }

      byDate.set(entry.date, {
        date: entry.date,
        entries: [entry],
        hours: entry.hours,
      });
    }

    return {
      groups: Array.from(byDate.values()).sort((a, b) =>
        b.date.localeCompare(a.date),
      ),
      totalHours: hours,
    };
  }, [absences]);

  return (
    <section className="rounded-lg border border-border/50 bg-card/30">
      <div className="flex items-center justify-between border-b border-border/40 px-4 py-3">
        <div className="flex items-center gap-2.5">
          <CalendarX2 className="h-4 w-4 text-muted-foreground" />
          <h2 className="font-display text-lg font-semibold">Absences</h2>
        </div>
        {!isLoading && absences.length > 0 && (
          <span className="font-mono text-xs text-muted-foreground">
            {absences.length} {absences.length === 1 ? "entry" : "entries"}{" "}
            &middot;{" "}
            <span className="font-semibold text-foreground">{totalHours}h</span>
          </span>
        )}
      </div>

      <div className="p-4">
        {isLoading ? (
          <div className="flex items-center justify-center py-6 text-sm text-muted-foreground">
            Loading absences...
          </div>
        ) : groups.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 py-6 text-center">
            <CalendarX2 className="h-8 w-8 text-muted-foreground/40" />
            <p className="text-sm text-muted-foreground">
              No absences in this range.
            </p>
          </div>
        ) : (
          <div className="space-y-5">
            {groups.map((group) => (
              <div key={group.date} className="space-y-2">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    {dayjs(group.date).format("ddd, MMM D")}
                  </span>
                  <div className="h-px flex-1 bg-border/50" />
                  <span className="font-mono text-xs text-muted-foreground">
                    {group.hours}h
                  </span>
                </div>

                <div className="space-y-1.5">
                  {group.entries.map((entry) => {
                    const Icon = absenceTypeIcons[entry.absenceType];
                    const accent = absenceTypeAccent[entry.absenceType];

                    return (
                      <div
                        key={entry.absenceId}
                        className={cn(
                          "group flex items-center gap-3 rounded-md border-l-[3px] px-3 py-2.5",
                          accent,
                        )}
                      >
                        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-background/80">
                          <Icon className="h-3.5 w-3.5 text-muted-foreground" />
                        </div>

                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium leading-tight">
                              {entry.absenceTypeLabel}
                            </span>
                            {!entry.managed && (
                              <span className="rounded border border-border/60 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/70">
                                Read-only
                              </span>
                            )}
                          </div>
                          {(entry.child || entry.comment) && (
                            <p className="mt-0.5 truncate text-xs text-muted-foreground">
                              {[entry.child, entry.comment]
                                .filter(Boolean)
                                .join(" \u00B7 ")}
                            </p>
                          )}
                        </div>

                        <div className="flex shrink-0 items-center gap-1">
                          {entry.deletable && (
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              className="h-7 w-7 shrink-0 text-muted-foreground opacity-0 transition-opacity hover:text-destructive group-hover:opacity-100"
                              onClick={() => setPendingDelete(entry)}
                              aria-label="Delete absence"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </Button>
                          )}
                          <span className="shrink-0 rounded-md bg-background/80 px-2 py-0.5 font-mono text-xs font-medium text-foreground/80">
                            {entry.hours}h
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <AbsenceDeleteDialog
        absence={pendingDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
      />
    </section>
  );
}
