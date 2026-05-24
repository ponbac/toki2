import { useMemo, useState } from "react";
import dayjs from "dayjs";
import type { AbsenceEntry, TimeEntry } from "@/lib/api/queries/time-tracking";
import { formatHoursAsHoursMinutes } from "@/lib/utils";
import { Clock } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { buildAbsenceDisplayRange } from "../-helpers/absence-display";
import {
  isAbsenceListEntry,
  isMergedTimeEntry,
  isWorkListEntry,
  type AbsenceListEntry,
  type MergedTimeEntry,
  type TimeEntriesListEntry,
  type WorkListEntry,
} from "../-helpers/time-entry-list-types";
import { AbsenceDeleteDialog } from "./absence-delete-dialog";
import { AbsenceEntryCard } from "./absence-entry-card";
import { buildProjectStyleMap } from "./colors";
import { TimeEntryEditContent } from "./time-entry-edit-content";
import { WorkEntryCard } from "./work-entry-card";

export function TimeEntriesList(props: {
  timeEntries: Array<TimeEntry>;
  absenceEntries: Array<AbsenceEntry>;
  mergeSameDay: boolean;
}) {
  const [editingEntryId, setEditingEntryId] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<AbsenceEntry | null>(null);

  const groupedEntries: Array<[string, Array<TimeEntriesListEntry>]> =
    useMemo(() => {
      const workGroups = groupTimeEntriesByDate(props.timeEntries);
      const workEntriesByDate = props.mergeSameDay
        ? mergeTimeEntriesByDate(workGroups)
        : workGroups;
      const absenceEntriesByDate = groupAbsencesByDate(props.absenceEntries);

      return combineEntriesByDate(workEntriesByDate, absenceEntriesByDate);
    }, [props.timeEntries, props.absenceEntries, props.mergeSameDay]);

  const projectStyleMap = useMemo(
    () => buildProjectStyleMap(props.timeEntries),
    [props.timeEntries],
  );

  const overlapMap = useMemo(() => {
    const totalVisible = groupedEntries.reduce(
      (sum, [, entries]) => sum + entries.filter(isWorkListEntry).length,
      0,
    );
    if (totalVisible > 250) return {};

    const result: Record<string, boolean> = {};

    groupedEntries.forEach(([, dayEntries]) => {
      const intervals = dayEntries
        .filter(isWorkListEntry)
        .flatMap((entry) => {
          if (isMergedTimeEntry(entry)) {
            return entry.timePeriods
              .map((p, i) =>
                p.startTime && p.endTime
                  ? {
                      id: `${entry.registrationId}-p${i}`,
                      start: new Date(p.startTime).getTime(),
                      end: new Date(p.endTime).getTime(),
                    }
                  : null,
              )
              .filter(Boolean) as Array<{
              id: string;
              start: number;
              end: number;
            }>;
          }

          return entry.startTime && entry.endTime
            ? [
                {
                  id: entry.registrationId,
                  start: new Date(entry.startTime).getTime(),
                  end: new Date(entry.endTime).getTime(),
                },
              ]
            : [];
        })
        .sort((a, b) => a.start - b.start);

      const dayOverlaps = new Set<string>();

      intervals.forEach((curr, idx) => {
        for (
          let j = idx + 1;
          j < intervals.length && intervals[j].start < curr.end;
          j++
        ) {
          const currEndMinute = Math.floor(curr.end / 60000);
          const nextStartMinute = Math.floor(intervals[j].start / 60000);
          if (nextStartMinute === currEndMinute) continue;

          dayOverlaps.add(curr.id);
          dayOverlaps.add(intervals[j].id);
        }
      });

      if (dayOverlaps.size > 1) {
        dayOverlaps.forEach((id) => {
          result[id] = true;
        });
      }
    });

    return result;
  }, [groupedEntries]);

  return (
    <div className="space-y-10">
      {groupedEntries.map(([dateKey, dayEntries], groupIndex) => {
        return (
          <motion.div
            key={dateKey}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.4, delay: groupIndex * 0.1 }}
          >
            <div className="mb-4 flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary">
                <span className="font-display text-lg font-bold">
                  {dayjs(dateKey).format("DD")}
                </span>
              </div>
              <div>
                <h2 className="font-display text-lg font-semibold leading-tight">
                  {dayjs(dateKey).format("dddd")}
                </h2>
                <p className="text-sm text-muted-foreground">
                  {dayjs(dateKey).format("MMMM YYYY")}
                </p>
              </div>
              <div className="ml-auto flex items-center gap-2 text-sm text-muted-foreground">
                <Clock className="h-4 w-4" />
                <span className="time-display">
                  {formatHoursAsHoursMinutes(
                    dayEntries.reduce((sum, e) => sum + e.hours, 0),
                  )}
                </span>
              </div>
            </div>

            <div className="space-y-3">
              <AnimatePresence mode="popLayout">
                {dayEntries.map((entry, entryIndex) => (
                  <motion.div
                    key={getEntryId(entry)}
                    layout
                    initial={{ opacity: 0, scale: 0.98 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.98 }}
                    transition={{ duration: 0.2, delay: entryIndex * 0.03 }}
                  >
                    {isAbsenceListEntry(entry) ? (
                      <AbsenceEntryCard
                        entry={entry}
                        onDelete={() => setPendingDelete(entry.absence)}
                      />
                    ) : editingEntryId === entry.registrationId ? (
                      <TimeEntryEditContent
                        entry={
                          isMergedTimeEntry(entry)
                            ? {
                                ...entry,
                                startTime: entry.timePeriods[0].startTime,
                                endTime: entry.timePeriods[0].endTime,
                              }
                            : entry
                        }
                        onSaved={() => setEditingEntryId(null)}
                        onCancel={() => setEditingEntryId(null)}
                        variant="inline"
                      />
                    ) : (
                      <WorkEntryCard
                        entry={entry}
                        onEdit={() => setEditingEntryId(entry.registrationId)}
                        overlapMap={overlapMap}
                        projectColor={
                          projectStyleMap.get(entry.projectName)?.color
                        }
                        ProjectIcon={
                          projectStyleMap.get(entry.projectName)?.Icon
                        }
                      />
                    )}
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>
          </motion.div>
        );
      })}
      <AbsenceDeleteDialog
        absence={pendingDelete}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
      />
    </div>
  );
}

function groupTimeEntriesByDate(entries: Array<TimeEntry>) {
  const groups: Record<string, Array<TimeEntry>> = {};
  entries.forEach((entry) => {
    const dateKey = dayjs(entry.date).format("YYYY-MM-DD");
    groups[dateKey] = [...(groups[dateKey] ?? []), entry];
  });
  return groups;
}

function groupAbsencesByDate(entries: Array<AbsenceEntry>) {
  const groups: Record<string, Array<AbsenceListEntry>> = {};
  entries.forEach((absence) => {
    const range = buildAbsenceDisplayRange(absence);
    const dateKey = dayjs(absence.date).format("YYYY-MM-DD");
    groups[dateKey] = [
      ...(groups[dateKey] ?? []),
      {
        kind: "absence",
        id: `absence:${absence.absenceId}`,
        hours: absence.hours,
        absence,
        sortTime: new Date(range.endTime).getTime(),
        startLabel: range.startLabel,
        endLabel: range.endLabel,
        isCapped: range.isCapped,
      },
    ];
  });
  return groups;
}

function mergeTimeEntriesByDate(
  groups: Record<string, Array<TimeEntry>>,
): Record<string, Array<MergedTimeEntry>> {
  return Object.fromEntries(
    Object.entries(groups).map(([dateKey, dayEntries]) => {
      const mergedByProjectActivityAndNote: Record<string, MergedTimeEntry> =
        {};

      dayEntries.forEach((entry) => {
        const key = `${entry.projectName}-${entry.activityName}-${entry.note}`;
        if (!mergedByProjectActivityAndNote[key]) {
          mergedByProjectActivityAndNote[key] = {
            ...entry,
            hours: 0,
            timePeriods: [],
          };
        }

        mergedByProjectActivityAndNote[key].hours += entry.hours;
        mergedByProjectActivityAndNote[key].timePeriods.push({
          startTime: entry.startTime,
          endTime: entry.endTime,
          status: entry.status,
        });
      });

      return [dateKey, Object.values(mergedByProjectActivityAndNote)];
    }),
  );
}

function combineEntriesByDate(
  workEntriesByDate: Record<string, Array<WorkListEntry>>,
  absenceEntriesByDate: Record<string, Array<AbsenceListEntry>>,
): Array<[string, Array<TimeEntriesListEntry>]> {
  const allDateKeys = new Set([
    ...Object.keys(workEntriesByDate),
    ...Object.keys(absenceEntriesByDate),
  ]);

  return Array.from(allDateKeys)
    .map((dateKey) => {
      const entries: Array<TimeEntriesListEntry> = [
        ...(workEntriesByDate[dateKey] ?? []),
        ...(absenceEntriesByDate[dateKey] ?? []),
      ].sort((a, b) => getEntrySortTime(b) - getEntrySortTime(a));

      return [dateKey, entries] as [string, Array<TimeEntriesListEntry>];
    })
    .sort(([a], [b]) => new Date(b).getTime() - new Date(a).getTime());
}

function getEntryId(entry: TimeEntriesListEntry) {
  return isAbsenceListEntry(entry) ? entry.id : entry.registrationId;
}

function getEntrySortTime(entry: TimeEntriesListEntry) {
  if (isAbsenceListEntry(entry)) return entry.sortTime;
  if (isMergedTimeEntry(entry)) {
    return entry.timePeriods.reduce((max, period) => {
      return period.endTime
        ? Math.max(max, new Date(period.endTime).getTime())
        : max;
    }, 0);
  }
  return entry.endTime ? new Date(entry.endTime).getTime() : 0;
}
