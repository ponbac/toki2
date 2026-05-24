import { memo } from "react";
import dayjs from "dayjs";
import { useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangleIcon,
  Briefcase,
  ChevronRight,
  LockIcon,
  PencilIcon,
  PlayIcon,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  timeTrackingQueries,
  type TimeEntry,
} from "@/lib/api/queries/time-tracking";
import { cn, formatHoursAsHoursMinutes } from "@/lib/utils";
import {
  isMergedTimeEntry,
  type MergedTimeEntry,
} from "../-helpers/time-entry-list-types";
import { useStartAgainTimer } from "../-helpers/use-start-again-timer";
import { withAlpha } from "./colors";
import { EntryCardFrame } from "./entry-card-frame";

export function WorkEntryCard(props: {
  entry: TimeEntry | MergedTimeEntry;
  onEdit: () => void;
  overlapMap: Record<string, boolean>;
  projectColor?: string;
  ProjectIcon?: LucideIcon;
}) {
  const queryClient = useQueryClient();
  const entry = props.entry;
  const isMerged = isMergedTimeEntry(entry);
  const isLocked = isMerged
    ? entry.timePeriods.every((p) => p.status !== "open")
    : entry.status !== "open";

  const Icon = isLocked ? LockIcon : (props.ProjectIcon ?? Briefcase);
  const timeRange = renderTimeRange({
    entry,
    overlapMap: props.overlapMap,
  });

  return (
    <EntryCardFrame className={cn(isLocked && "bg-muted/30")} showHoverOverlay>
      <div className="flex flex-col items-center">
        <div
          className={cn(
            "flex h-10 w-10 shrink-0 items-center justify-center rounded-lg",
            isLocked && "bg-muted text-muted-foreground",
          )}
          style={
            !isLocked && props.projectColor
              ? {
                  backgroundColor: withAlpha(props.projectColor, 0.15),
                  color: props.projectColor,
                }
              : !isLocked
                ? {
                    backgroundColor: "hsl(var(--primary) / 0.1)",
                    color: "hsl(var(--primary))",
                  }
                : undefined
          }
        >
          <Icon className="h-4 w-4" />
        </div>
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <h3 className="truncate font-semibold leading-tight">
              {entry.projectName}
            </h3>
            <p className="text-sm text-muted-foreground">
              {entry.activityName}
            </p>
          </div>

          <div className="shrink-0">
            <div
              className="rounded-lg px-3 py-1.5 text-sm font-semibold transition-opacity group-hover:opacity-0"
              style={
                props.projectColor
                  ? {
                      backgroundColor: withAlpha(props.projectColor, 0.15),
                      color: props.projectColor,
                    }
                  : {
                      backgroundColor: "hsl(var(--primary) / 0.1)",
                      color: "hsl(var(--primary))",
                    }
              }
            >
              <span className="time-display">
                {formatHoursAsHoursMinutes(entry.hours)}
              </span>
            </div>
            <div className="absolute right-4 top-4 flex items-center gap-1 rounded-lg border border-border/50 bg-card p-1 opacity-0 shadow-sm transition-opacity group-hover:opacity-100">
              {!isLocked && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={props.onEdit}
                        onMouseEnter={() => {
                          void queryClient.prefetchQuery(
                            timeTrackingQueries.listActivities(entry.projectId),
                          );
                        }}
                        onFocus={() => {
                          void queryClient.prefetchQuery(
                            timeTrackingQueries.listActivities(entry.projectId),
                          );
                        }}
                        disabled={isMerged && entry.timePeriods.length > 1}
                        className="h-7 w-7 rounded-md p-0 hover:bg-primary/10 hover:text-primary"
                      >
                        <PencilIcon className="h-3.5 w-3.5" />
                      </Button>
                    </span>
                  </TooltipTrigger>
                  <TooltipContent>
                    {isMerged && entry.timePeriods.length > 1
                      ? "Unmerge to edit"
                      : "Edit entry"}
                  </TooltipContent>
                </Tooltip>
              )}
              <StartAgainButton
                note={entry.note ?? ""}
                projectId={entry.projectId}
                projectName={entry.projectName}
                activityId={entry.activityId}
                activityName={entry.activityName}
              />
            </div>
          </div>
        </div>

        {entry.note && (
          <p className="mt-2 line-clamp-2 font-mono text-sm text-foreground/80">
            {entry.note}
          </p>
        )}

        {timeRange ? <div className="mt-2">{timeRange}</div> : null}
      </div>
    </EntryCardFrame>
  );
}

const StartAgainButton = memo(function StartAgainButton(props: {
  note: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
}) {
  const { isStarting, startAgain } = useStartAgainTimer();

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => startAgain(props)}
          disabled={isStarting}
          className="h-7 w-7 rounded-md p-0 hover:bg-primary/10 hover:text-primary"
        >
          <PlayIcon className="h-3.5 w-3.5" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>Start again</TooltipContent>
    </Tooltip>
  );
});

function renderTimeRange(props: {
  entry: TimeEntry | MergedTimeEntry;
  overlapMap: Record<string, boolean>;
}) {
  if (isMergedTimeEntry(props.entry)) {
    const mergedPeriodsWithTimes = props.entry.timePeriods.reduce<
      Array<{
        index: number;
        startTime: string;
        endTime: string;
      }>
    >((acc, period, index) => {
      if (period.startTime && period.endTime) {
        acc.push({
          index,
          startTime: period.startTime,
          endTime: period.endTime,
        });
      }
      return acc;
    }, []);

    if (mergedPeriodsWithTimes.length === 0) return null;

    if (mergedPeriodsWithTimes.length === 1) {
      const period = mergedPeriodsWithTimes[0];
      const periodId = `${props.entry.registrationId}-p${period.index}`;
      const isOverlap = props.overlapMap[periodId];
      return (
        <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <span className="time-display">
            {dayjs(period.startTime).format("HH:mm")}
          </span>
          <ChevronRight className="h-3 w-3" />
          <span className="time-display">
            {dayjs(period.endTime).format("HH:mm")}
          </span>
          {isOverlap && <OverlapWarning />}
        </div>
      );
    }

    return (
      <div className="flex max-h-20 flex-col gap-0.5 overflow-hidden text-sm text-muted-foreground">
        {mergedPeriodsWithTimes.slice(0, 3).map((period) => {
          const periodId = `${props.entry.registrationId}-p${period.index}`;
          const isOverlap = props.overlapMap[periodId];
          return (
            <div key={periodId} className="flex items-center gap-1">
              <span className="time-display text-xs">
                {dayjs(period.startTime).format("HH:mm")} -{" "}
                {dayjs(period.endTime).format("HH:mm")}
              </span>
              {isOverlap && <OverlapWarning className="h-3 w-3" />}
            </div>
          );
        })}
        {mergedPeriodsWithTimes.length > 3 && (
          <span className="text-xs text-muted-foreground/70">
            +{mergedPeriodsWithTimes.length - 3} more
          </span>
        )}
      </div>
    );
  }

  if (!props.entry.endTime) return null;

  return (
    <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
      <span className="time-display">
        {props.entry.startTime && dayjs(props.entry.startTime).format("HH:mm")}
      </span>
      <ChevronRight className="h-3 w-3" />
      <span className="time-display">
        {props.entry.endTime && dayjs(props.entry.endTime).format("HH:mm")}
      </span>
      {props.overlapMap[props.entry.registrationId] && <OverlapWarning />}
    </div>
  );
}

function OverlapWarning(props: { className?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <AlertTriangleIcon
          className={cn("h-4 w-4 text-amber-500", props.className)}
        />
      </TooltipTrigger>
      <TooltipContent>Overlapping time interval</TooltipContent>
    </Tooltip>
  );
}
