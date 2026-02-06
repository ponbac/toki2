import React, { useMemo, useState, useEffect } from "react";
import { TimeEntry } from "@/lib/api/queries/milltime";
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
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import { COLORS, withAlpha, buildProjectColorMap } from "./colors";
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import { useMilltimeTimer } from "@/hooks/useMilltimeStore";
import { toast } from "sonner";

type TimelineViewProps = {
  timeEntries: TimeEntry[];
  dateRange: { from: string; to: string };
};

type TimelineMode = "day" | "week";

const DEFAULT_START_HOUR = 8;
const DEFAULT_END_HOUR = 17;
const MOCK_START_HOUR = 8; // untimed entries stack from 08:00
const HOUR_HEIGHT_PX = 80;
const MIN_BLOCK_PX = 36;


type PositionedEntry = TimeEntry & {
  topPx: number;
  heightPx: number;
  column: number;
  totalColumns: number;
  mockTimes: boolean;
};

function positionEntries(
  dayEntries: TimeEntry[],
  gridStartHour: number,
): PositionedEntry[] {
  if (dayEntries.length === 0) return [];

  const withTimes: TimeEntry[] = [];
  const withoutTimes: TimeEntry[] = [];

  dayEntries.forEach((entry) => {
    if (entry.startTime && entry.endTime) {
      withTimes.push(entry);
    } else {
      withoutTimes.push(entry);
    }
  });

  const positioned: PositionedEntry[] = [];

  // Position entries with real times
  withTimes.forEach((entry) => {
    const start = parseISO(entry.startTime!);
    const end = parseISO(entry.endTime!);
    const startH =
      start.getHours() + start.getMinutes() / 60 - gridStartHour;
    const endH = end.getHours() + end.getMinutes() / 60 - gridStartHour;

    positioned.push({
      ...entry,
      topPx: Math.max(0, startH * HOUR_HEIGHT_PX),
      heightPx: Math.max(MIN_BLOCK_PX, (endH - startH) * HOUR_HEIGHT_PX),
      column: 0,
      totalColumns: 1,
      mockTimes: false,
    });
  });

  // For entries without times, stack sequentially from MOCK_START_HOUR
  if (withoutTimes.length > 0) {
    let currentHour = MOCK_START_HOUR - gridStartHour;
    withoutTimes.forEach((entry) => {
      const duration = Math.max(entry.hours, 0.5);
      positioned.push({
        ...entry,
        topPx: Math.max(0, currentHour * HOUR_HEIGHT_PX),
        heightPx: Math.max(MIN_BLOCK_PX, duration * HOUR_HEIGHT_PX),
        column: 0,
        totalColumns: 1,
        mockTimes: true,
      });
      currentHour += duration + 0.04;
    });
  }

  // Resolve overlaps
  const sorted = positioned.sort((a, b) => a.topPx - b.topPx);
  for (let i = 0; i < sorted.length; i++) {
    const overlapping = sorted.filter(
      (other, j) =>
        j < i &&
        other.topPx < sorted[i].topPx + sorted[i].heightPx &&
        other.topPx + other.heightPx > sorted[i].topPx,
    );
    if (overlapping.length > 0) {
      const usedColumns = new Set(overlapping.map((o) => o.column));
      let col = 0;
      while (usedColumns.has(col)) col++;
      sorted[i].column = col;
      const maxCol = Math.max(col, ...overlapping.map((o) => o.column)) + 1;
      overlapping.forEach((o) => (o.totalColumns = maxCol));
      sorted[i].totalColumns = maxCol;
    }
  }

  return sorted;
}

/** Scan entries to find the earliest start and latest end, with 30 min padding */
function computeGridBounds(entries: TimeEntry[]): {
  startHour: number;
  endHour: number;
} {
  if (entries.length === 0) {
    return { startHour: DEFAULT_START_HOUR, endHour: DEFAULT_END_HOUR };
  }

  let earliest = Infinity;
  let latest = -Infinity;
  // Group untimed entries by date in the same pass
  const untimedByDate = new Map<string, TimeEntry[]>();

  entries.forEach((entry) => {
    if (entry.startTime && entry.endTime) {
      const s = parseISO(entry.startTime);
      earliest = Math.min(earliest, s.getHours() + s.getMinutes() / 60);
      const e = parseISO(entry.endTime);
      latest = Math.max(latest, e.getHours() + e.getMinutes() / 60);
    } else {
      const arr = untimedByDate.get(entry.date) || [];
      arr.push(entry);
      untimedByDate.set(entry.date, arr);
    }
  });

  // Account for untimed entries (mock-positioned from MOCK_START_HOUR)
  if (untimedByDate.size > 0) {
    earliest = Math.min(earliest, MOCK_START_HOUR);
    untimedByDate.forEach((dayEntries) => {
      let mockEnd = MOCK_START_HOUR;
      dayEntries.forEach((e) => {
        mockEnd += Math.max(e.hours, 0.5) + 0.04;
      });
      latest = Math.max(latest, mockEnd);
    });
  }

  if (!isFinite(earliest)) earliest = DEFAULT_START_HOUR;
  if (!isFinite(latest)) latest = DEFAULT_END_HOUR;

  return {
    startHour: Math.max(0, earliest - 0.5),
    endHour: Math.min(24, latest + 0.5),
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
          className="absolute right-0 -translate-y-1/2 pr-2 text-[11px] text-muted-foreground/70 time-display"
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

function NowIndicator({
  date,
  startHour,
}: {
  date: Date;
  startHour: number;
}) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    if (!isToday(date)) return;
    const id = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(id);
  }, [date]);

  if (!isToday(date)) return null;

  const currentHour =
    now.getHours() + now.getMinutes() / 60 - startHour;

  if (currentHour < 0) return null;

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
  entry: PositionedEntry;
  isWeekView: boolean;
}) {
  const { mutateAsync: startStandaloneAsync, isPending: isStarting } =
    milltimeMutations.useStartStandaloneTimer();
  const { mutateAsync: editStandaloneAsync } =
    milltimeMutations.useEditStandaloneTimer();
  const { state: timerState } = useMilltimeTimer();

  const handleStartAgain = (e: React.MouseEvent) => {
    e.stopPropagation();
    const isStandaloneActive = timerState === "running";

    if (isStandaloneActive) {
      editStandaloneAsync({
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

    startStandaloneAsync({ userNote: entry.note ?? "" })
      .then(() =>
        editStandaloneAsync({
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
      onClick={handleStartAgain}
      disabled={isStarting}
      className={cn(
        "absolute right-1 top-1 z-20 flex h-6 w-6 items-center justify-center rounded-md border border-border/50 bg-card/95 opacity-0 shadow-sm backdrop-blur-sm transition-opacity",
        "hover:bg-primary/10 hover:text-primary",
        "group-hover/block:opacity-100",
        isWeekView && "h-5 w-5 right-0.5 top-0.5",
      )}
    >
      <PlayIcon className={cn("h-3 w-3", isWeekView && "h-2.5 w-2.5")} />
    </button>
  );
});

function TimelineBlock({
  entry,
  color,
  isWeekView,
}: {
  entry: PositionedEntry;
  color: string;
  isWeekView: boolean;
}) {
  const columnWidth = 100 / entry.totalColumns;
  const leftPercent = entry.column * columnWidth;

  const startTime = entry.startTime
    ? format(parseISO(entry.startTime), "HH:mm")
    : null;
  const endTime = entry.endTime
    ? format(parseISO(entry.endTime), "HH:mm")
    : null;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <motion.div
          initial={{ opacity: 0, scale: 0.96 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ duration: 0.25 }}
          className={cn(
            "group/block absolute z-10 cursor-pointer overflow-hidden rounded-lg border transition-all duration-200",
            "hover:z-30 hover:shadow-lg",
            entry.mockTimes && "border-dashed opacity-85",
          )}
          style={{
            top: entry.topPx,
            height: entry.heightPx,
            left: `calc(${leftPercent}% + 6px)`,
            width: `calc(${columnWidth}% - 12px)`,
            backgroundColor: withAlpha(color, 0.18),
            borderColor: withAlpha(color, 0.4),
            boxShadow: `0 1px 4px ${withAlpha(color, 0.15)}`,
          }}
        >
          <div
            className="absolute inset-y-0 left-0 w-[3px]"
            style={{ backgroundColor: color }}
          />
          {/* Play button overlay on hover */}
          <PlayButton entry={entry} isWeekView={isWeekView} />
          <div className="flex h-full flex-col justify-start overflow-hidden pl-2.5 pr-1.5 py-1">
            <p
              className={cn(
                "truncate font-semibold leading-tight",
                isWeekView ? "text-[10px]" : "text-xs",
              )}
              style={{ color }}
            >
              {entry.projectName}
            </p>
            {entry.heightPx > 38 && (
              <p
                className={cn(
                  "truncate text-muted-foreground",
                  isWeekView ? "text-[9px]" : "text-[11px]",
                )}
              >
                {entry.activityName}
              </p>
            )}
            {!isWeekView && entry.heightPx > 76 && entry.note && (
              <p className="mt-0.5 line-clamp-2 text-[10px] font-mono text-muted-foreground/80">
                {entry.note}
              </p>
            )}
            {entry.heightPx > 48 && (
              <p
                className={cn(
                  "mt-auto time-display text-muted-foreground/60",
                  isWeekView ? "text-[9px]" : "text-[10px]",
                )}
              >
                {formatHoursAsHoursMinutes(entry.hours)}
              </p>
            )}
          </div>
        </motion.div>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        className="max-w-xs rounded-lg border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm"
      >
        <div className="space-y-1.5">
          <div className="flex items-center gap-2">
            <div
              className="h-2.5 w-2.5 rounded-full"
              style={{ backgroundColor: color }}
            />
            <p className="font-semibold">{entry.projectName}</p>
          </div>
          <p className="text-sm text-muted-foreground">{entry.activityName}</p>
          {entry.note && (
            <p className="text-sm font-mono text-foreground/80">{entry.note}</p>
          )}
          <div className="flex items-center gap-3 pt-1 text-sm">
            <span className="time-display font-medium">
              {formatHoursAsHoursMinutes(entry.hours)}
            </span>
            {startTime && endTime && (
              <span className="time-display text-muted-foreground">
                {startTime} — {endTime}
              </span>
            )}
          </div>
          {entry.mockTimes && (
            <p className="text-[10px] text-muted-foreground/60 italic">
              Estimated position (no start/end time)
            </p>
          )}
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

function DayColumn({
  date,
  entries,
  colorMap,
  gridHeight,
  startHour,
  endHour,
  isWeekView,
  isOnly,
}: {
  date: Date;
  entries: PositionedEntry[];
  colorMap: Map<string, string>;
  gridHeight: number;
  startHour: number;
  endHour: number;
  isWeekView: boolean;
  isOnly: boolean;
}) {
  const dayTotal = entries.reduce((sum, e) => sum + e.hours, 0);
  const today = isToday(date);

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
          "flex flex-col items-center gap-0.5 border-b border-border/30 px-2 py-2.5",
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
            today
              ? "bg-primary text-primary-foreground"
              : "text-foreground",
          )}
        >
          {format(date, "d")}
        </span>
        {dayTotal > 0 && (
          <span className="text-[11px] time-display text-muted-foreground">
            {formatHoursAsHoursMinutes(dayTotal)}
          </span>
        )}
      </div>

      {/* Timeline area — explicit height from computed grid bounds */}
      <div className="relative" style={{ height: gridHeight }}>
        <HourGridLines startHour={startHour} endHour={endHour} />
        <NowIndicator date={date} startHour={startHour} />
        {entries.map((entry) => (
          <TimelineBlock
            key={entry.registrationId}
            entry={entry}
            color={colorMap.get(entry.projectName) || COLORS[0]}
            isWeekView={isWeekView}
          />
        ))}
      </div>
    </div>
  );
}

export function TimelineView({ timeEntries, dateRange }: TimelineViewProps) {
  const [mode, setMode] = useState<TimelineMode>("week");

  const rangeFrom = useMemo(() => parseISO(dateRange.from), [dateRange.from]);
  const rangeTo = useMemo(() => parseISO(dateRange.to), [dateRange.to]);

  // Week view: track which week is displayed
  const [currentWeekStart, setCurrentWeekStart] = useState(() => {
    const from = parseISO(dateRange.from);
    const to = parseISO(dateRange.to);
    const today = startOfDay(new Date());
    const target = today >= from && today <= to ? today : from;
    return startOfWeek(target, { weekStartsOn: 1 });
  });

  // Day view: track selected date
  const [selectedDate, setSelectedDate] = useState(() => {
    const from = parseISO(dateRange.from);
    const to = parseISO(dateRange.to);
    const today = startOfDay(new Date());
    return today >= from && today <= to ? today : from;
  });

  // Reset when dateRange changes
  useEffect(() => {
    const from = parseISO(dateRange.from);
    const to = parseISO(dateRange.to);
    const today = startOfDay(new Date());
    const target = today >= from && today <= to ? today : from;
    setCurrentWeekStart(startOfWeek(target, { weekStartsOn: 1 }));
    setSelectedDate(target);
  }, [dateRange.from, dateRange.to]);

  const colorMap = useMemo(
    () => buildProjectColorMap(timeEntries),
    [timeEntries],
  );

  // Pre-group entries by date so we don't filter per day
  const entriesByDate = useMemo(() => {
    const map = new Map<string, TimeEntry[]>();
    timeEntries.forEach((e) => {
      const key = e.date.slice(0, 10);
      const arr = map.get(key) || [];
      arr.push(e);
      map.set(key, arr);
    });
    return map;
  }, [timeEntries]);

  const weekDays = useMemo(() => {
    const days: Date[] = [];
    for (let i = 0; i < 7; i++) {
      const day = addDays(currentWeekStart, i);
      const key = format(day, "yyyy-MM-dd");
      // Always include Mon-Fri; include Sat/Sun only if they have entries
      if (i < 5 || entriesByDate.has(key)) {
        days.push(day);
      }
    }
    return days;
  }, [currentWeekStart, entriesByDate]);

  // Multi-week navigation
  const isMultiWeek = useMemo(() => {
    const firstWeek = startOfWeek(rangeFrom, { weekStartsOn: 1 });
    const lastWeek = startOfWeek(rangeTo, { weekStartsOn: 1 });
    return firstWeek.getTime() !== lastWeek.getTime();
  }, [rangeFrom, rangeTo]);

  const canGoPrevWeek = useMemo(
    () => addDays(currentWeekStart, -7) >= startOfWeek(rangeFrom, { weekStartsOn: 1 }),
    [currentWeekStart, rangeFrom],
  );

  const canGoNextWeek = useMemo(
    () => addDays(currentWeekStart, 7) <= startOfWeek(rangeTo, { weekStartsOn: 1 }),
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

  const selectedDayKey = useMemo(
    () => format(selectedDate, "yyyy-MM-dd"),
    [selectedDate],
  );

  // Only use entries visible in current view for grid bounds
  const visibleEntries = useMemo(() => {
    if (mode === "week") {
      const entries: TimeEntry[] = [];
      weekDays.forEach((day) => {
        const key = format(day, "yyyy-MM-dd");
        const dayEntries = entriesByDate.get(key);
        if (dayEntries) entries.push(...dayEntries);
      });
      return entries;
    }
    return entriesByDate.get(selectedDayKey) ?? [];
  }, [mode, weekDays, entriesByDate, selectedDayKey]);

  const { startHour, endHour } = useMemo(
    () => computeGridBounds(visibleEntries),
    [visibleEntries],
  );

  const positionedByDay = useMemo(() => {
    const days = mode === "week" ? weekDays : [selectedDate];
    const map = new Map<string, PositionedEntry[]>();
    days.forEach((day) => {
      const key = format(day, "yyyy-MM-dd");
      const dayEntries = entriesByDate.get(key) ?? [];
      map.set(key, positionEntries(dayEntries, startHour));
    });
    return map;
  }, [entriesByDate, mode, weekDays, selectedDate, startHour]);

  const totalHours = endHour - startHour;
  const gridHeight = totalHours * HOUR_HEIGHT_PX;

  const totalHoursInView = useMemo(() => {
    let total = 0;
    positionedByDay.forEach((entries) => {
      total += entries.reduce((sum, e) => sum + e.hours, 0);
    });
    return total;
  }, [positionedByDay]);

  return (
    <div className="card-elevated overflow-hidden rounded-2xl">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border/30 px-5 py-3">
        <div className="flex items-center gap-3">
          <Clock className="h-4 w-4 text-primary" />
          <h3 className="font-display text-base font-semibold">Timeline</h3>
          {totalHoursInView > 0 && (
            <span className="text-sm text-muted-foreground time-display">
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
                {format(currentWeekStart, "MMM d")} – {format(weekDays[weekDays.length - 1], "MMM d")}
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
      <ScrollArea
        style={{ height: Math.min(720, gridHeight + 80) }}
      >
        <div
          className="flex"
          style={{ minWidth: mode === "week" ? 700 : 400 }}
        >
          {/* Hour labels */}
          <div className="relative shrink-0">
            <div className="h-[76px]" />
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
                    const key = format(day, "yyyy-MM-dd");
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
                  />
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
    </div>
  );
}
