import React, { useMemo, useState, useEffect, useRef } from "react";
import {
  AttestLevel,
  TimeEntry,
  timeTrackingQueries,
} from "@/lib/api/queries/time-tracking";
import { cn, formatHoursAsHoursMinutes } from "@/lib/utils";
import {
  format,
  parseISO,
  startOfWeek,
  addDays,
  isToday,
  startOfDay,
} from "date-fns";
import { motion, AnimatePresence } from "framer-motion";
import { ChevronLeft, ChevronRight, Clock, PlayIcon } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import { COLORS, withAlpha, buildProjectColorMap } from "./colors";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { toast } from "sonner";
import { TimeEntryEditDialog } from "./time-entry-edit-dialog";
import {
  ActiveTimerBlock,
  buildActiveTimerIntervalsByDay,
  buildActiveTimerSegments,
  computeActiveTimerHourBounds,
  type ActiveTimerSegment,
} from "./active-timer-block";
import {
  SavedTimelineCardBody,
  SavedTimelineCardTooltipBody,
} from "./timeline-card-content";
import { buildTimelineCardText } from "./timeline-card-text";
import {
  MICRO_LANE_X_OFFSET_PX,
  layoutDayEntries,
  type TimelineLaidOutEntry,
} from "./timeline-layout";

type TimelineViewProps = {
  timeEntries: TimeEntry[];
  dateRange: { from: string; to: string };
};

type TimelineMode = "day" | "week";

const DEFAULT_START_HOUR = 8;
const DEFAULT_END_HOUR = 17;
const MIN_VISIBLE_END_HOUR = 20;
const HOUR_HEIGHT_PX = 96;
const TIMELINE_CHROME_HEIGHT_PX = 80;
const DAY_KEY_FORMAT = "yyyy-MM-dd";
const WEEK_STARTS_ON = 1 as const;
const DAY_HEADER_LAYOUT_CLASS = "flex flex-col items-center gap-0.5 px-2 py-2.5";
const DAY_HEADER_TOTAL_CLASS = "time-display text-[11px] text-muted-foreground";
const BLOCK_SIDE_INSET_PX = 6;

function toDayKey(date: Date) {
  return format(date, DAY_KEY_FORMAT);
}

function getInRangeDate(fromIso: string, toIso: string) {
  const from = parseISO(fromIso);
  const to = parseISO(toIso);
  const today = startOfDay(new Date());
  return today >= from && today <= to ? today : from;
}

/** Scan entries to find the earliest start and latest end, with 30 min padding */
function computeGridBounds(
  entries: TimeEntry[],
  activeTimerBounds?: { earliestHour: number; latestHour: number } | null,
): {
  startHour: number;
  endHour: number;
} {
  let earliest = Infinity;
  let latest = -Infinity;

  entries.forEach((entry) => {
    if (entry.startTime && entry.endTime) {
      const s = parseISO(entry.startTime);
      earliest = Math.min(earliest, s.getHours() + s.getMinutes() / 60);
      const e = parseISO(entry.endTime);
      latest = Math.max(latest, e.getHours() + e.getMinutes() / 60);
    }
  });

  if (!isFinite(earliest)) earliest = DEFAULT_START_HOUR;
  if (!isFinite(latest)) latest = DEFAULT_END_HOUR;

  if (activeTimerBounds) {
    earliest = Math.min(earliest, activeTimerBounds.earliestHour);
    latest = Math.max(latest, activeTimerBounds.latestHour);
  }

  return {
    startHour: Math.max(0, earliest - 0.5),
    endHour: Math.max(MIN_VISIBLE_END_HOUR, Math.min(24, latest + 0.5)),
  };
}

function HourLabels({
  startHour,
  endHour,
}: {
  startHour: number;
  endHour: number;
}) {
  const hours = [];
  for (let h = Math.ceil(startHour); h <= Math.floor(endHour); h++) {
    hours.push(h);
  }

  return (
    <div className="relative h-full w-12 shrink-0 select-none">
      {hours.map((hour) => (
        <div
          key={hour}
          className="time-display absolute right-0 -translate-y-1/2 pr-2 text-[11px] text-muted-foreground/70"
          style={{ top: (hour - startHour) * HOUR_HEIGHT_PX }}
        >
          {String(hour).padStart(2, "0")}
        </div>
      ))}
    </div>
  );
}

function HourGridLines({
  startHour,
  endHour,
}: {
  startHour: number;
  endHour: number;
}) {
  const lines = [];
  for (let h = Math.ceil(startHour); h <= Math.floor(endHour); h++) {
    lines.push(
      <div
        key={h}
        className="absolute left-0 right-0 border-t border-border/30"
        style={{ top: (h - startHour) * HOUR_HEIGHT_PX }}
      />,
    );
  }
  return <>{lines}</>;
}

function NowIndicator({ date, startHour }: { date: Date; startHour: number }) {
  const [now, setNow] = useState(() => new Date());
  const isTodayColumn = isToday(date);

  useEffect(() => {
    if (!isTodayColumn) return;
    // Depend on stable day-identity boolean so interval isn't restarted by recreated Date objects.
    const id = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(id);
  }, [isTodayColumn]);

  const currentHour = now.getHours() + now.getMinutes() / 60 - startHour;
  if (!isTodayColumn || currentHour < 0) return null;

  return (
    <div
      className="absolute left-0 right-0 z-20 flex items-center"
      style={{ top: currentHour * HOUR_HEIGHT_PX }}
    >
      <div className="h-2.5 w-2.5 -translate-x-1/2 rounded-full bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.5)]" />
      <div className="h-[2px] flex-1 bg-red-500/80" />
    </div>
  );
}

const PlayButton = React.memo(function PlayButton({
  entry,
  isWeekView,
}: {
  entry: TimelineLaidOutEntry;
  isWeekView: boolean;
}) {
  const { mutateAsync: startTimerAsync, isPending: isStarting } =
    timeTrackingMutations.useStartTimer();
  const { mutateAsync: editTimerAsync } = timeTrackingMutations.useEditTimer();
  const { state: timerState } = useTimeTrackingTimer();

  const handleStartAgain = (e: React.MouseEvent) => {
    e.stopPropagation();
    const isTimerActive = timerState === "running";

    if (isTimerActive) {
      editTimerAsync({
        userNote: entry.note ?? "",
        projectId: entry.projectId,
        projectName: entry.projectName,
        activityId: entry.activityId,
        activityName: entry.activityName,
      })
        .then(() => toast.success("Timer updated"))
        .catch(() => toast.error("Failed to update timer"));
      return;
    }

    startTimerAsync({ userNote: entry.note ?? "" })
      .then(() =>
        editTimerAsync({
          projectId: entry.projectId,
          projectName: entry.projectName,
          activityId: entry.activityId,
          activityName: entry.activityName,
        }),
      )
      .then(() => toast.success("Timer started"))
      .catch(() => toast.error("Failed to start timer"));
  };

  return (
    <button
      type="button"
      onClick={handleStartAgain}
      disabled={isStarting}
      className={cn(
        "absolute right-1 top-1 z-20 flex h-6 w-6 items-center justify-center rounded-md border border-border/50 bg-card/95 opacity-0 shadow-sm backdrop-blur-sm transition-opacity",
        "hover:bg-primary/10 hover:text-primary",
        "group-hover/block:opacity-100",
        isWeekView && "right-0.5 top-0.5 h-5 w-5",
      )}
    >
      <PlayIcon className={cn("h-3 w-3", isWeekView && "h-2.5 w-2.5")} />
    </button>
  );
});

const TimelineBlock = React.memo(function TimelineBlock({
  entry,
  dayContentWidthPx,
  color,
  isWeekView,
  isEditable,
  onClick,
}: {
  entry: TimelineLaidOutEntry;
  dayContentWidthPx: number | null;
  color: string;
  isWeekView: boolean;
  isEditable: boolean;
  onClick?: () => void;
}) {
  const isMicro = entry.visualVariant === "micro";
  const columnWidth = 100 / entry.totalColumns;
  const leftPercent = entry.column * columnWidth;
  const microOffsetPx = isMicro ? entry.microLane * MICRO_LANE_X_OFFSET_PX : 0;
  const blockLeftInsetPx = BLOCK_SIDE_INSET_PX + microOffsetPx;
  const blockWidthValue = isMicro
    ? `max(34px, calc(${columnWidth}% - ${BLOCK_SIDE_INSET_PX * 2 + microOffsetPx}px))`
    : `calc(${columnWidth}% - ${BLOCK_SIDE_INSET_PX * 2}px)`;
  const blockWidthPx =
    dayContentWidthPx === null
      ? null
      : Math.max(
          isMicro ? 34 : 0,
          dayContentWidthPx / entry.totalColumns -
            (BLOCK_SIDE_INSET_PX * 2 + microOffsetPx),
        );

  const startTime = entry.startTime
    ? format(parseISO(entry.startTime), "HH:mm")
    : null;
  const endTime = entry.endTime
    ? format(parseISO(entry.endTime), "HH:mm")
    : null;
  const text = buildTimelineCardText({
    projectName: entry.projectName,
    activityName: entry.activityName,
    note: entry.note,
  });
  const timeRangeLabel = startTime && endTime ? `${startTime} — ${endTime}` : null;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <motion.div
          initial={{ opacity: 0, scale: 0.96 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.25 }}
          onClick={isEditable ? onClick : undefined}
          className={cn(
            "group/block absolute z-10 rounded-lg border transition-all duration-200",
            isEditable
              ? "cursor-pointer hover:z-30 hover:shadow-lg"
              : "cursor-default",
            isMicro ? "overflow-visible" : "overflow-hidden",
          )}
          style={{
            top: entry.topPx,
            height: entry.visualHeightPx,
            left: `calc(${leftPercent}% + ${blockLeftInsetPx}px)`,
            width: blockWidthValue,
            backgroundColor: withAlpha(color, 0.18),
            borderColor: withAlpha(color, 0.4),
            boxShadow: `0 1px 4px ${withAlpha(color, 0.15)}`,
          }}
        >
          {isMicro && (
            <>
              <div
                className="pointer-events-none absolute left-[5px] top-0 w-[2px] rounded-full"
                style={{
                  height: Math.max(2, entry.durationPx),
                  backgroundColor: withAlpha(color, 0.7),
                }}
              />
              <div
                className="pointer-events-none absolute left-[3px] h-[2px] w-[6px]"
                style={{
                  top: Math.max(1, entry.durationPx),
                  backgroundColor: withAlpha(color, 0.8),
                }}
              />
            </>
          )}
          <div
            className="absolute inset-y-0 left-0 w-[3px]"
            style={{ backgroundColor: color }}
          />
          {!isMicro && <PlayButton entry={entry} isWeekView={isWeekView} />}
          <SavedTimelineCardBody
            text={text}
            heightPx={entry.visualHeightPx}
            widthPx={blockWidthPx}
            color={color}
            hours={entry.hours}
            isWeekView={isWeekView}
            forceProjectOnly={isMicro}
          />
        </motion.div>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        className="max-w-xs rounded-lg border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm"
      >
        <SavedTimelineCardTooltipBody
          text={text}
          color={color}
          hours={entry.hours}
          timeRangeLabel={timeRangeLabel}
          footerHint={!isEditable ? "Locked entry" : undefined}
        />
      </TooltipContent>
    </Tooltip>
  );
});

function DayColumn({
  date,
  entries,
  colorMap,
  gridHeight,
  startHour,
  endHour,
  isWeekView,
  isOnly,
  activeTimerSegment,
  onEntryClick,
}: {
  date: Date;
  entries: TimelineLaidOutEntry[];
  colorMap: Map<string, string>;
  gridHeight: number;
  startHour: number;
  endHour: number;
  isWeekView: boolean;
  isOnly: boolean;
  activeTimerSegment?: ActiveTimerSegment | null;
  onEntryClick: (entry: TimelineLaidOutEntry) => void;
}) {
  const dayTotal = entries.reduce((sum, e) => sum + e.hours, 0);
  const today = isToday(date);
  const hasDayTotal = dayTotal > 0;
  const timelineAreaRef = useRef<HTMLDivElement | null>(null);
  const [dayContentWidthPx, setDayContentWidthPx] = useState<number | null>(null);

  useEffect(() => {
    const element = timelineAreaRef.current;
    if (!element) return;

    const updateWidth = () => {
      const next = element.getBoundingClientRect().width;
      setDayContentWidthPx(Number.isFinite(next) ? next : null);
    };

    updateWidth();

    if (typeof ResizeObserver === "undefined") return;

    const observer = new ResizeObserver(() => updateWidth());
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  return (
    <div
      className={cn(
        "flex flex-1 flex-col",
        isWeekView && "min-w-0",
        !isOnly && "border-l border-border/20 first:border-l-0",
      )}
    >
      {/* Day header */}
      <div
        className={cn(
          DAY_HEADER_LAYOUT_CLASS,
          "border-b border-border/30",
          today && "bg-primary/5",
        )}
      >
        <span
          className={cn(
            "text-xs font-medium uppercase tracking-wider",
            today ? "text-primary" : "text-muted-foreground",
          )}
        >
          {format(date, isWeekView ? "EEE" : "EEEE")}
        </span>
        <span
          className={cn(
            "flex h-8 w-8 items-center justify-center rounded-full text-sm font-bold",
            today ? "bg-primary text-primary-foreground" : "text-foreground",
          )}
        >
          {format(date, "d")}
        </span>
        <span
          className={cn(
            DAY_HEADER_TOTAL_CLASS,
            !hasDayTotal && "invisible",
          )}
        >
          {hasDayTotal ? formatHoursAsHoursMinutes(dayTotal) : "0m"}
        </span>
      </div>

      {/* Timeline area — explicit height from computed grid bounds */}
      <div ref={timelineAreaRef} className="relative" style={{ height: gridHeight }}>
        <HourGridLines startHour={startHour} endHour={endHour} />
        <NowIndicator date={date} startHour={startHour} />
        {activeTimerSegment && (
          <ActiveTimerBlock
            segment={activeTimerSegment}
            isWeekView={isWeekView}
            dayContentWidthPx={dayContentWidthPx}
          />
        )}
        {entries.map((entry) => (
          <TimelineBlock
            key={entry.registrationId}
            entry={entry}
            dayContentWidthPx={dayContentWidthPx}
            color={colorMap.get(entry.projectName) || COLORS[0]}
            isWeekView={isWeekView}
            isEditable={entry.attestLevel === AttestLevel.None}
            onClick={() => onEntryClick(entry)}
          />
        ))}
      </div>
    </div>
  );
}

function TimelineHeaderSpacer() {
  return (
    <div
      aria-hidden
      className={cn(DAY_HEADER_LAYOUT_CLASS, "border-b border-transparent")}
    >
      <span className="invisible text-xs font-medium uppercase tracking-wider">
        Mon
      </span>
      <span className="invisible flex h-8 w-8 items-center justify-center rounded-full text-sm font-bold">
        1
      </span>
      <span className={cn(DAY_HEADER_TOTAL_CLASS, "invisible")}>
        0m
      </span>
    </div>
  );
}

export function TimelineView({ timeEntries, dateRange }: TimelineViewProps) {
  const { state: timerState } = useTimeTrackingTimer();

  const [mode, setMode] = useState<TimelineMode>("week");
  const [editingEntry, setEditingEntry] = useState<TimeEntry | null>(null);
  const [activeTimerNow, setActiveTimerNow] = useState(() => new Date());

  const { data: activeTimer } = useQuery({
    ...timeTrackingQueries.getTimer(),
    enabled: timerState === "running" || timerState === undefined,
    refetchInterval:
      timerState === "running" || timerState === undefined ? 60_000 : false,
    select: (data) => data.timer,
  });
  const normalizedActiveTimer = activeTimer ?? null;

  const rangeFrom = useMemo(() => parseISO(dateRange.from), [dateRange.from]);
  const rangeTo = useMemo(() => parseISO(dateRange.to), [dateRange.to]);

  // Week view: track which week is displayed
  const [currentWeekStart, setCurrentWeekStart] = useState(() => {
    const target = getInRangeDate(dateRange.from, dateRange.to);
    return startOfWeek(target, { weekStartsOn: WEEK_STARTS_ON });
  });

  // Day view: track selected date
  const [selectedDate, setSelectedDate] = useState(() =>
    getInRangeDate(dateRange.from, dateRange.to),
  );

  // Reset when dateRange changes
  useEffect(() => {
    const target = getInRangeDate(dateRange.from, dateRange.to);
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setCurrentWeekStart(startOfWeek(target, { weekStartsOn: WEEK_STARTS_ON }));
    setSelectedDate(target);
  }, [dateRange.from, dateRange.to]);

  const colorMap = useMemo(
    () => buildProjectColorMap(timeEntries),
    [timeEntries],
  );

  // Pre-group timed entries by date so timeline only renders precise placements.
  const entriesByDate = useMemo(() => {
    const map = new Map<string, TimeEntry[]>();
    timeEntries.forEach((e) => {
      if (!e.startTime || !e.endTime) return;
      const key = e.date.slice(0, 10);
      const arr = map.get(key) || [];
      arr.push(e);
      map.set(key, arr);
    });
    return map;
  }, [timeEntries]);

  const weekCandidates = useMemo(
    () => Array.from({ length: 7 }, (_, index) => addDays(currentWeekStart, index)),
    [currentWeekStart],
  );

  const weekDays = useMemo(() => {
    const activeTimerWeekIntervalsByDay = buildActiveTimerIntervalsByDay({
      timer: normalizedActiveTimer,
      now: activeTimerNow,
      days: weekCandidates,
      toDayKey,
    });

    return weekCandidates.filter((day, index) => {
      const key = toDayKey(day);
      // Always include Mon-Fri; include Sat/Sun only if they have entries or active timer overlap
      return (
        index < 5 ||
        entriesByDate.has(key) ||
        activeTimerWeekIntervalsByDay.has(key)
      );
    });
  }, [entriesByDate, normalizedActiveTimer, activeTimerNow, weekCandidates]);

  // Multi-week navigation
  const isMultiWeek = useMemo(() => {
    const firstWeek = startOfWeek(rangeFrom, { weekStartsOn: WEEK_STARTS_ON });
    const lastWeek = startOfWeek(rangeTo, { weekStartsOn: WEEK_STARTS_ON });
    return firstWeek.getTime() !== lastWeek.getTime();
  }, [rangeFrom, rangeTo]);

  const canGoPrevWeek = useMemo(
    () =>
      addDays(currentWeekStart, -7) >=
      startOfWeek(rangeFrom, { weekStartsOn: WEEK_STARTS_ON }),
    [currentWeekStart, rangeFrom],
  );

  const canGoNextWeek = useMemo(
    () =>
      addDays(currentWeekStart, 7) <=
      startOfWeek(rangeTo, { weekStartsOn: WEEK_STARTS_ON }),
    [currentWeekStart, rangeTo],
  );

  // Day navigation
  const canGoPrevDay = useMemo(
    () => addDays(selectedDate, -1) >= rangeFrom,
    [selectedDate, rangeFrom],
  );

  const canGoNextDay = useMemo(
    () => addDays(selectedDate, 1) <= rangeTo,
    [selectedDate, rangeTo],
  );

  const selectedDayKey = useMemo(() => toDayKey(selectedDate), [selectedDate]);
  const activeTimerStartTime = normalizedActiveTimer?.startTime ?? null;

  useEffect(() => {
    if (!activeTimerStartTime) return;
    const id = setInterval(() => setActiveTimerNow(new Date()), 30_000);
    return () => clearInterval(id);
  }, [activeTimerStartTime]);

  const visibleDays = useMemo(
    () => (mode === "week" ? weekDays : [selectedDate]),
    [mode, weekDays, selectedDate],
  );

  // Only use entries visible in current view for grid bounds
  const visibleEntries = useMemo(() => {
    const entries: TimeEntry[] = [];
    visibleDays.forEach((day) => {
      const key = toDayKey(day);
      const dayEntries = entriesByDate.get(key);
      if (dayEntries) entries.push(...dayEntries);
    });
    return entries;
  }, [visibleDays, entriesByDate]);

  const activeTimerIntervalsByDay = useMemo(
    () =>
      buildActiveTimerIntervalsByDay({
        timer: normalizedActiveTimer,
        now: activeTimerNow,
        days: visibleDays,
        toDayKey,
      }),
    [normalizedActiveTimer, activeTimerNow, visibleDays],
  );

  const activeTimerHourBounds = useMemo(
    () => computeActiveTimerHourBounds(activeTimerIntervalsByDay),
    [activeTimerIntervalsByDay],
  );

  const { startHour, endHour } = useMemo(
    () => computeGridBounds(visibleEntries, activeTimerHourBounds),
    [visibleEntries, activeTimerHourBounds],
  );

  const positionedByDay = useMemo(() => {
    const map = new Map<string, TimelineLaidOutEntry[]>();
    visibleDays.forEach((day) => {
      const key = toDayKey(day);
      const dayEntries = entriesByDate.get(key) ?? [];
      map.set(
        key,
        layoutDayEntries({
          dayEntries,
          gridStartHour: startHour,
          gridEndHour: endHour,
          hourHeightPx: HOUR_HEIGHT_PX,
        }),
      );
    });
    return map;
  }, [entriesByDate, visibleDays, startHour, endHour]);

  const totalHours = endHour - startHour;
  const gridHeight = totalHours * HOUR_HEIGHT_PX;

  const activeTimerSegmentsByDay = useMemo(
    () =>
      buildActiveTimerSegments({
        timer: normalizedActiveTimer,
        intervalsByDay: activeTimerIntervalsByDay,
        now: activeTimerNow,
        startHour,
        hourHeightPx: HOUR_HEIGHT_PX,
        gridHeight,
        colorMap,
      }),
    [
      normalizedActiveTimer,
      activeTimerIntervalsByDay,
      activeTimerNow,
      startHour,
      gridHeight,
      colorMap,
    ],
  );

  const totalHoursInView = useMemo(() => {
    let total = 0;
    positionedByDay.forEach((entries) => {
      total += entries.reduce((sum, e) => sum + e.hours, 0);
    });
    return total;
  }, [positionedByDay]);

  const handleEntryClick = (entry: TimelineLaidOutEntry) => {
    if (entry.attestLevel !== AttestLevel.None) return;
    setEditingEntry(entry);
  };

  return (
    <div className="card-elevated overflow-hidden rounded-2xl">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border/30 px-5 py-3">
        <div className="flex items-center gap-3">
          <Clock className="h-4 w-4 text-primary" />
          <h3 className="font-display text-base font-semibold">Timeline</h3>
          {totalHoursInView > 0 && (
            <span className="time-display text-sm text-muted-foreground">
              {formatHoursAsHoursMinutes(totalHoursInView)}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {mode === "week" && isMultiWeek && (
            <div className="mr-2 flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setCurrentWeekStart((prev) => addDays(prev, -7))}
                disabled={!canGoPrevWeek}
                className="h-7 w-7 p-0"
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
              <span className="min-w-[130px] text-center text-sm font-medium">
                {format(currentWeekStart, "MMM d")} –{" "}
                {format(weekDays[weekDays.length - 1], "MMM d")}
              </span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setCurrentWeekStart((prev) => addDays(prev, 7))}
                disabled={!canGoNextWeek}
                className="h-7 w-7 p-0"
              >
                <ChevronRight className="h-4 w-4" />
              </Button>
            </div>
          )}
          {mode === "day" && (
            <div className="mr-2 flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setSelectedDate((prev) => addDays(prev, -1))}
                disabled={!canGoPrevDay}
                className="h-7 w-7 p-0"
              >
                <ChevronLeft className="h-4 w-4" />
              </Button>
              <span className="min-w-[100px] text-center text-sm font-medium">
                {format(selectedDate, "EEE, MMM d")}
              </span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setSelectedDate((prev) => addDays(prev, 1))}
                disabled={!canGoNextDay}
                className="h-7 w-7 p-0"
              >
                <ChevronRight className="h-4 w-4" />
              </Button>
            </div>
          )}

          <div className="flex overflow-hidden rounded-lg border border-border/50 bg-muted/30">
            <button
              type="button"
              onClick={() => setMode("day")}
              className={cn(
                "px-3 py-1.5 text-xs font-medium transition-all",
                mode === "day"
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              Day
            </button>
            <button
              type="button"
              onClick={() => setMode("week")}
              className={cn(
                "px-3 py-1.5 text-xs font-medium transition-all",
                mode === "week"
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              Week
            </button>
          </div>
        </div>
      </div>

      {/* Legend */}
      <div className="flex flex-wrap gap-x-4 gap-y-1.5 border-b border-border/20 px-5 py-2.5">
        {[...colorMap.entries()].map(([project, color]) => (
          <Tooltip key={project}>
            <TooltipTrigger asChild>
              <div className="flex items-center gap-1.5">
                <div
                  className="h-2 w-2 shrink-0 rounded-full"
                  style={{ backgroundColor: color }}
                />
                <span className="max-w-[200px] truncate text-[11px] text-muted-foreground">
                  {project}
                </span>
              </div>
            </TooltipTrigger>
            <TooltipContent side="bottom" className="text-xs">
              {project}
            </TooltipContent>
          </Tooltip>
        ))}
      </div>

      {/* Timeline body — needs definite height for Radix ScrollArea viewport to scroll */}
      <ScrollArea style={{ height: gridHeight + TIMELINE_CHROME_HEIGHT_PX }}>
        <div className="flex" style={{ minWidth: mode === "week" ? 700 : 400 }}>
          {/* Hour labels */}
          <div className="relative shrink-0">
            <TimelineHeaderSpacer />
            <div className="relative" style={{ height: gridHeight }}>
              <HourLabels startHour={startHour} endHour={endHour} />
            </div>
          </div>

          {/* Day columns */}
          <div className="flex flex-1">
            <AnimatePresence mode="wait">
              {mode === "week" ? (
                <motion.div
                  key={`week-${format(currentWeekStart, "yyyy-MM-dd")}`}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.2 }}
                  className="flex flex-1"
                >
                  {weekDays.map((day) => {
                    const key = toDayKey(day);
                    return (
                      <DayColumn
                        key={key}
                        date={day}
                        entries={positionedByDay.get(key) || []}
                        colorMap={colorMap}
                        gridHeight={gridHeight}
                        startHour={startHour}
                        endHour={endHour}
                        isWeekView={true}
                        isOnly={false}
                        activeTimerSegment={
                          activeTimerSegmentsByDay.get(key) ?? null
                        }
                        onEntryClick={handleEntryClick}
                      />
                    );
                  })}
                </motion.div>
              ) : (
                <motion.div
                  key={`day-${selectedDayKey}`}
                  initial={{ opacity: 0, x: 20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20 }}
                  transition={{ duration: 0.2 }}
                  className="flex flex-1"
                >
                  <DayColumn
                    date={selectedDate}
                    entries={positionedByDay.get(selectedDayKey) || []}
                    colorMap={colorMap}
                    gridHeight={gridHeight}
                    startHour={startHour}
                    endHour={endHour}
                    isWeekView={false}
                    isOnly={true}
                    activeTimerSegment={
                      activeTimerSegmentsByDay.get(selectedDayKey) ?? null
                    }
                    onEntryClick={handleEntryClick}
                  />
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
      <TimeEntryEditDialog
        entry={editingEntry}
        open={editingEntry !== null}
        onOpenChange={(open) => {
          if (!open) setEditingEntry(null);
        }}
        onSaved={() => setEditingEntry(null)}
      />
    </div>
  );
}
