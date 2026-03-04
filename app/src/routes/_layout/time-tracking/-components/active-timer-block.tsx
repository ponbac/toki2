/* eslint-disable react-refresh/only-export-components */
import type { CSSProperties } from "react";
import { addDays, format, parseISO, startOfDay } from "date-fns";
import { TimerResponse } from "@/lib/api/queries/time-tracking";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { COLORS, withAlpha } from "./colors";
import {
  ActiveTimelineCardBody,
  ActiveTimelineCardTooltipBody,
} from "./timeline-card-content";
import { buildTimelineCardText } from "./timeline-card-text";

const HOURS_PER_DAY = 24;

export type ActiveTimerHourBounds = {
  earliestHour: number;
  latestHour: number;
};

export type ActiveTimerSegment = {
  topPx: number;
  heightPx: number;
  color: string;
  projectName: string | null;
  activityName: string | null;
  note: string;
  segmentStart: Date;
  segmentEnd: Date;
  isCurrent: boolean;
  hours: number;
};

export type ActiveTimerDayInterval = {
  start: Date;
  end: Date;
  dayStart: Date;
  dayEnd: Date;
};

export function ActiveTimerBlock({
  segment,
  isWeekView,
  dayContentWidthPx,
}: {
  segment: ActiveTimerSegment;
  isWeekView: boolean;
  dayContentWidthPx: number | null;
}) {
  const text = buildTimelineCardText({
    projectName: segment.projectName,
    activityName: segment.activityName,
    note: segment.note,
  });
  const timeRangeLabel = `${format(segment.segmentStart, "HH:mm")} — ${
    segment.isCurrent ? "Now" : format(segment.segmentEnd, "HH:mm")
  }`;
  const cardWidthPx =
    dayContentWidthPx === null ? null : Math.max(0, dayContentWidthPx - 12);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          className="timeline-active-timer-block absolute left-0 right-0 z-[9] mx-[6px] cursor-help rounded-lg"
          style={
            {
              top: segment.topPx,
              height: segment.heightPx,
              backgroundColor: withAlpha(segment.color, 0.1),
              boxShadow: `0 6px 14px ${withAlpha(segment.color, 0.14)}`,
              "--active-shine-color": withAlpha(segment.color, 0.98),
              "--active-shine-glow": withAlpha(segment.color, 0.45),
            } as CSSProperties
          }
          aria-label="Active timer (not saved)"
        >
          <ActiveTimelineCardBody
            text={text}
            heightPx={segment.heightPx}
            widthPx={cardWidthPx}
            color={segment.color}
            hours={segment.hours}
            isWeekView={isWeekView}
            projectColor={withAlpha(segment.color, 0.96)}
            className="relative z-[2]"
          />
        </div>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        className="max-w-xs rounded-lg border-border/50 bg-card/95 p-3 shadow-elevated backdrop-blur-sm"
      >
        <ActiveTimelineCardTooltipBody
          text={text}
          color={segment.color}
          hours={segment.hours}
          timeRangeLabel={timeRangeLabel}
        />
      </TooltipContent>
    </Tooltip>
  );
}

function getHourInDay(date: Date, dayStart: Date, dayEnd: Date) {
  if (date.getTime() === dayEnd.getTime()) {
    return HOURS_PER_DAY;
  }

  return (date.getTime() - dayStart.getTime()) / 3_600_000;
}

function getClampedDayInterval(
  timerStart: Date,
  now: Date,
  day: Date,
): ActiveTimerDayInterval | null {
  const dayStart = startOfDay(day);
  const dayEnd = addDays(dayStart, 1);
  const start = timerStart > dayStart ? timerStart : dayStart;
  const end = now < dayEnd ? now : dayEnd;

  if (end <= start) return null;

  return { start, end, dayStart, dayEnd };
}

export function buildActiveTimerIntervalsByDay({
  timer,
  now,
  days,
  toDayKey,
}: {
  timer: TimerResponse | null;
  now: Date;
  days: Date[];
  toDayKey: (day: Date) => string;
}): Map<string, ActiveTimerDayInterval> {
  const intervalsByDay = new Map<string, ActiveTimerDayInterval>();
  if (!timer || days.length === 0) return intervalsByDay;

  const timerStart = parseISO(timer.startTime);
  if (Number.isNaN(timerStart.getTime()) || now <= timerStart) return intervalsByDay;

  days.forEach((day) => {
    const interval = getClampedDayInterval(timerStart, now, day);
    if (!interval) return;

    intervalsByDay.set(toDayKey(day), interval);
  });

  return intervalsByDay;
}

export function computeActiveTimerHourBounds(
  intervalsByDay: Map<string, ActiveTimerDayInterval>,
): ActiveTimerHourBounds | null {
  if (intervalsByDay.size === 0) return null;

  let earliestHour = Infinity;
  let latestHour = -Infinity;

  intervalsByDay.forEach((interval) => {
    earliestHour = Math.min(
      earliestHour,
      getHourInDay(interval.start, interval.dayStart, interval.dayEnd),
    );
    latestHour = Math.max(
      latestHour,
      getHourInDay(interval.end, interval.dayStart, interval.dayEnd),
    );
  });

  if (!isFinite(earliestHour) || !isFinite(latestHour)) return null;

  return { earliestHour, latestHour };
}

export function buildActiveTimerSegments({
  timer,
  intervalsByDay,
  now,
  startHour,
  hourHeightPx,
  gridHeight,
  colorMap,
}: {
  timer: TimerResponse | null;
  intervalsByDay: Map<string, ActiveTimerDayInterval>;
  now: Date;
  startHour: number;
  hourHeightPx: number;
  gridHeight: number;
  colorMap: Map<string, string>;
}): Map<string, ActiveTimerSegment> {
  const segmentsByDay = new Map<string, ActiveTimerSegment>();
  if (!timer || intervalsByDay.size === 0 || gridHeight <= 0) return segmentsByDay;

  const color = timer.projectName
    ? colorMap.get(timer.projectName) || COLORS[0]
    : COLORS[0];

  intervalsByDay.forEach((interval, dayKey) => {
    const startHourInDay = getHourInDay(interval.start, interval.dayStart, interval.dayEnd);
    const endHourInDay = getHourInDay(interval.end, interval.dayStart, interval.dayEnd);

    const rawTop = (startHourInDay - startHour) * hourHeightPx;
    const rawBottom = (endHourInDay - startHour) * hourHeightPx;

    const clampedTop = Math.max(0, Math.min(rawTop, gridHeight));
    const clampedBottom = Math.max(0, Math.min(rawBottom, gridHeight));
    if (clampedBottom <= clampedTop) return;

    const topPx = clampedTop;
    const heightPx = Math.max(1, clampedBottom - clampedTop);

    segmentsByDay.set(dayKey, {
      topPx,
      heightPx,
      color,
      projectName: timer.projectName,
      activityName: timer.activityName,
      note: timer.note ?? "",
      segmentStart: interval.start,
      segmentEnd: interval.end,
      isCurrent: interval.end.getTime() === now.getTime(),
      hours: (interval.end.getTime() - interval.start.getTime()) / 3_600_000,
    });
  });

  return segmentsByDay;
}
