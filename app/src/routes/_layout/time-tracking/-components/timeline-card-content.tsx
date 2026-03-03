import { cn, formatHoursAsHoursMinutes } from "@/lib/utils";
import type { TimelineCardText } from "./timeline-card-text";

type BodyConfig = {
  showProjectAt: number;
  showPrimaryAt: number;
  showSecondaryAt: number;
  showDurationAt: number;
  twoLinePrimaryAt: number;
  compactAt?: number;
  spacingClass: string;
  projectTextClass: string;
  primaryTextClass: string;
  secondaryTextClass: string;
  durationTextClass: string;
};

const SAVED_DAY_BODY: BodyConfig = {
  showProjectAt: 0,
  showPrimaryAt: 24,
  showSecondaryAt: 90,
  showDurationAt: 48,
  twoLinePrimaryAt: 90,
  spacingClass: "py-1 pl-2.5 pr-1.5",
  projectTextClass: "text-xs",
  primaryTextClass: "text-[12px]",
  secondaryTextClass: "-mt-px text-[11px]",
  durationTextClass: "text-[10px] text-muted-foreground/60",
};

const SAVED_WEEK_BODY: BodyConfig = {
  showProjectAt: 0,
  showPrimaryAt: 24,
  showSecondaryAt: 96,
  showDurationAt: 48,
  twoLinePrimaryAt: Infinity,
  spacingClass: "py-1 pl-2.5 pr-1.5",
  projectTextClass: "text-[10px]",
  primaryTextClass: "text-[10px]",
  secondaryTextClass: "-mt-px text-[9.5px]",
  durationTextClass: "text-[9px] text-muted-foreground/60",
};

const ACTIVE_DAY_BODY: BodyConfig = {
  showProjectAt: 22,
  showPrimaryAt: 46,
  showSecondaryAt: 76,
  showDurationAt: 60,
  twoLinePrimaryAt: 84,
  compactAt: 40,
  spacingClass: "px-2.5 py-1",
  projectTextClass: "text-[11px]",
  primaryTextClass: "text-[10px]",
  secondaryTextClass: "mt-px text-[10px]",
  durationTextClass: "text-[10px] text-muted-foreground/70",
};

const ACTIVE_WEEK_BODY: BodyConfig = {
  ...ACTIVE_DAY_BODY,
};

function TimelineCardBodyBase({
  text,
  heightPx,
  color,
  hours,
  projectColor,
  className,
  config,
}: {
  text: TimelineCardText;
  heightPx: number;
  color: string;
  hours: number;
  projectColor?: string;
  className?: string;
  config: BodyConfig;
}) {
  const isCompact = config.compactAt !== undefined && heightPx < config.compactAt;
  const showProjectLabel = heightPx >= config.showProjectAt;
  const showPrimaryDetail = heightPx > config.showPrimaryAt;
  const showSecondaryDetail = text.hasNote && heightPx > config.showSecondaryAt;
  const showDuration = heightPx >= config.showDurationAt;
  const showTwoLinePrimary = heightPx >= config.twoLinePrimaryAt;

  return (
    <div
      className={cn(
        "flex h-full min-h-0 flex-col justify-start overflow-hidden",
        isCompact ? "px-2 py-0.5" : config.spacingClass,
        className,
      )}
    >
      {showProjectLabel && (
        <p
          className={cn("truncate font-semibold leading-tight", config.projectTextClass)}
          style={{ color: projectColor ?? color }}
        >
          {text.projectLabel}
        </p>
      )}
      {showPrimaryDetail && (
        <p
          className={cn(
            "mt-0.5 leading-snug",
            text.hasNote ? "text-foreground/85" : "text-muted-foreground",
            config.primaryTextClass,
            showTwoLinePrimary ? "line-clamp-2" : "line-clamp-1",
          )}
        >
          {text.primaryDetail}
        </p>
      )}
      {showSecondaryDetail && (
        <p
          className={cn(
            "truncate leading-tight text-muted-foreground",
            config.secondaryTextClass,
          )}
        >
          {text.activityLabel}
        </p>
      )}
      {showDuration && (
        <p className={cn("time-display mt-auto", config.durationTextClass)}>
          {formatHoursAsHoursMinutes(hours)}
        </p>
      )}
    </div>
  );
}

function TimelineCardTooltipBodyBase({
  text,
  color,
  hours,
  timeRangeLabel,
  heading,
  footerHint,
  detailOrder,
}: {
  text: TimelineCardText;
  color: string;
  hours: number;
  timeRangeLabel?: string | null;
  heading?: string;
  footerHint?: string;
  detailOrder: "note-first" | "activity-first";
}) {
  return (
    <div className="space-y-1.5">
      {heading && (
        <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          {heading}
        </p>
      )}
      <div className="flex items-center gap-2">
        <div className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: color }} />
        <p className="font-semibold">{text.projectLabel}</p>
      </div>
      {detailOrder === "activity-first" && (
        <p className="text-sm text-muted-foreground">{text.activityLabel}</p>
      )}
      {text.hasNote && <p className="text-sm text-foreground/90">{text.note}</p>}
      {detailOrder === "note-first" && (
        <p className="text-sm text-muted-foreground">{text.activityLabel}</p>
      )}
      <div className="flex items-center gap-3 pt-1 text-sm">
        <span className="time-display font-medium">
          {formatHoursAsHoursMinutes(hours)}
        </span>
        {timeRangeLabel && <span className="time-display text-muted-foreground">{timeRangeLabel}</span>}
      </div>
      {footerHint && <p className="text-[10px] text-muted-foreground/60">{footerHint}</p>}
    </div>
  );
}

export function SavedTimelineCardBody({
  text,
  heightPx,
  color,
  hours,
  isWeekView,
}: {
  text: TimelineCardText;
  heightPx: number;
  color: string;
  hours: number;
  isWeekView: boolean;
}) {
  return (
    <TimelineCardBodyBase
      text={text}
      heightPx={heightPx}
      color={color}
      hours={hours}
      config={isWeekView ? SAVED_WEEK_BODY : SAVED_DAY_BODY}
    />
  );
}

export function ActiveTimelineCardBody({
  text,
  heightPx,
  color,
  hours,
  isWeekView,
  projectColor,
  className,
}: {
  text: TimelineCardText;
  heightPx: number;
  color: string;
  hours: number;
  isWeekView: boolean;
  projectColor: string;
  className?: string;
}) {
  return (
    <TimelineCardBodyBase
      text={text}
      heightPx={heightPx}
      color={color}
      hours={hours}
      projectColor={projectColor}
      className={className}
      config={isWeekView ? ACTIVE_WEEK_BODY : ACTIVE_DAY_BODY}
    />
  );
}

export function SavedTimelineCardTooltipBody({
  text,
  color,
  hours,
  timeRangeLabel,
  footerHint,
}: {
  text: TimelineCardText;
  color: string;
  hours: number;
  timeRangeLabel?: string | null;
  footerHint?: string;
}) {
  return (
    <TimelineCardTooltipBodyBase
      text={text}
      color={color}
      hours={hours}
      timeRangeLabel={timeRangeLabel}
      footerHint={footerHint}
      detailOrder="note-first"
    />
  );
}

export function ActiveTimelineCardTooltipBody({
  text,
  color,
  hours,
  timeRangeLabel,
}: {
  text: TimelineCardText;
  color: string;
  hours: number;
  timeRangeLabel: string;
}) {
  return (
    <TimelineCardTooltipBodyBase
      text={text}
      color={color}
      hours={hours}
      timeRangeLabel={timeRangeLabel}
      heading="Running timer (not saved)"
      detailOrder="activity-first"
    />
  );
}
