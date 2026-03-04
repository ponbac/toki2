import { cn, formatHoursAsHoursMinutes } from "@/lib/utils";
import type { TimelineCardText } from "./timeline-card-text";

type BodyConfig = {
  spacingClass: string;
  compactSpacingClass: string;
  projectTextClass: string;
  primaryTextClass: string;
  secondaryTextClass: string;
  durationTextClass: string;
};

const SAVED_DAY_BODY: BodyConfig = {
  spacingClass: "py-1 pl-2.5 pr-1.5",
  compactSpacingClass: "px-2 py-0.5",
  projectTextClass: "text-xs",
  primaryTextClass: "text-[12px]",
  secondaryTextClass: "-mt-px text-[11px]",
  durationTextClass: "text-[10px] text-muted-foreground/60",
};

const SAVED_WEEK_BODY: BodyConfig = {
  spacingClass: "py-1 pl-2.5 pr-1.5",
  compactSpacingClass: "px-2 py-0.5",
  projectTextClass: "text-[10px]",
  primaryTextClass: "text-[10px]",
  secondaryTextClass: "-mt-px text-[9.5px]",
  durationTextClass: "text-[9px] text-muted-foreground/60",
};

const ACTIVE_DAY_BODY: BodyConfig = {
  spacingClass: "px-2.5 py-1",
  compactSpacingClass: "px-2 py-0.5",
  projectTextClass: "text-xs",
  primaryTextClass: "text-[12px]",
  secondaryTextClass: "-mt-px text-[11px]",
  durationTextClass: "text-[10px] text-muted-foreground/70",
};

const ACTIVE_WEEK_BODY: BodyConfig = {
  ...ACTIVE_DAY_BODY,
  projectTextClass: "text-[10px]",
  primaryTextClass: "text-[10px]",
  secondaryTextClass: "-mt-px text-[9.5px]",
  durationTextClass: "text-[9px] text-muted-foreground/70",
};

type DensityMode =
  | "project_only"
  | "project_duration"
  | "project_primary"
  | "project_primary_duration"
  | "full";

function hasPrimaryDetail(mode: DensityMode) {
  return (
    mode === "project_primary" ||
    mode === "project_primary_duration" ||
    mode === "full"
  );
}

function hasDuration(mode: DensityMode) {
  return (
    mode === "project_duration" ||
    mode === "project_primary_duration" ||
    mode === "full"
  );
}

function resolveDensityMode({
  heightPx,
  widthPx,
  forceProjectOnly = false,
}: {
  heightPx: number;
  widthPx: number | null;
  forceProjectOnly?: boolean;
}): DensityMode {
  if (forceProjectOnly) return "project_only";

  if (heightPx < 34) {
    return "project_only";
  }

  // Width limits text density, but should not collapse medium-height cards to project-only.
  if (widthPx !== null) {
    if (widthPx < 120) {
      if (heightPx < 44) return "project_duration";
      if (heightPx < 56) return "project_primary";
      return "project_primary_duration";
    }
    if (widthPx < 180) {
      if (heightPx < 40) return "project_duration";
      if (heightPx < 52) return "project_primary";
      return heightPx >= 92 ? "full" : "project_primary_duration";
    }
    if (widthPx < 250) {
      if (heightPx < 44) return "project_primary";
      return heightPx >= 96 ? "full" : "project_primary_duration";
    }
    if (widthPx < 320 && heightPx < 96) {
      return "project_primary_duration";
    }
  }

  // In roomier columns, allow notes earlier so ~30 minute entries can show
  // useful context while still reserving full mode for taller cards.
  if (heightPx < 64) {
    return "project_primary";
  }
  if (heightPx < 96) {
    return "project_primary_duration";
  }
  return "full";
}

function TimelineCardBodyBase({
  text,
  heightPx,
  widthPx,
  color,
  hours,
  projectColor,
  className,
  forceProjectOnly,
  config,
}: {
  text: TimelineCardText;
  heightPx: number;
  widthPx: number | null;
  color: string;
  hours: number;
  projectColor?: string;
  className?: string;
  forceProjectOnly?: boolean;
  config: BodyConfig;
}) {
  const densityMode = resolveDensityMode({
    heightPx,
    widthPx,
    forceProjectOnly,
  });
  const isCompact =
    densityMode === "project_only" ||
    densityMode === "project_duration" ||
    densityMode === "project_primary";
  const showPrimaryDetail = hasPrimaryDetail(densityMode);
  const showSecondaryDetail = densityMode === "full" && text.hasNote;
  const showDuration = hasDuration(densityMode);
  const showTwoLinePrimary =
    densityMode === "full" &&
    heightPx >= 132 &&
    (widthPx === null || widthPx >= 380);

  return (
    <div
      className={cn(
        "flex h-full min-h-0 flex-col justify-start overflow-hidden",
        isCompact ? config.compactSpacingClass : config.spacingClass,
        className,
      )}
    >
      <p
        className={cn("truncate font-semibold leading-tight", config.projectTextClass)}
        style={{ color: projectColor ?? color }}
      >
        {text.projectLabel}
      </p>
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
  widthPx,
  color,
  hours,
  isWeekView,
  forceProjectOnly,
}: {
  text: TimelineCardText;
  heightPx: number;
  widthPx: number | null;
  color: string;
  hours: number;
  isWeekView: boolean;
  forceProjectOnly?: boolean;
}) {
  return (
    <TimelineCardBodyBase
      text={text}
      heightPx={heightPx}
      widthPx={widthPx}
      color={color}
      hours={hours}
      forceProjectOnly={forceProjectOnly}
      config={isWeekView ? SAVED_WEEK_BODY : SAVED_DAY_BODY}
    />
  );
}

export function ActiveTimelineCardBody({
  text,
  heightPx,
  widthPx,
  color,
  hours,
  isWeekView,
  projectColor,
  className,
}: {
  text: TimelineCardText;
  heightPx: number;
  widthPx: number | null;
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
      widthPx={widthPx}
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
