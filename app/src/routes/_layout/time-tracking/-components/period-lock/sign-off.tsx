import React from "react";
import { addMonths, addWeeks, format, parseISO } from "date-fns";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  AlertTriangle,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  LockIcon,
  LockOpenIcon,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import {
  capitalize,
  dayIssues,
  daysIn,
  formatRange,
  hoursShort,
  isLocked,
  isoWeekOf,
  lockedThrough,
  lockLevel,
  monthRange,
  rangesWithStatus,
  scopeAction,
  splitAtMonthEnd,
  summarize,
  toIsoDate,
  weekRange,
  type LockDay,
  type LockRange,
} from "@/lib/period-lock/lock-periods";
import {
  useDueLock,
  useLockActions,
  useLockDays,
} from "@/lib/period-lock/use-lock-days";
import { cn, formatHoursMinutes } from "@/lib/utils";
import { LockGlyph } from "./lock-glyph";

const SETTLE_MS = 1700;

type Kind = "due" | "week" | "month";
type Selection = {
  kind: Kind;
  anchor: string;
  part: LockRange | null;
  name?: string;
};
type ShowDay = (date: string) => void;
type BandTone = "neutral" | "due" | "overdue" | "month-overdue" | "done";

const BAND_STYLES: Record<BandTone, string> = {
  neutral: "bg-muted/30",
  due: "bg-primary/10",
  overdue: "bg-primary text-primary-foreground",
  "month-overdue": "bg-destructive text-destructive-foreground",
  done: "bg-[color-mix(in_srgb,var(--viz-3)_16%,transparent)]",
};

const BADGE_STYLES: Record<BandTone, string> = {
  neutral: "bg-foreground/10",
  due: "bg-primary/15 text-primary",
  overdue: "bg-primary-foreground/15",
  "month-overdue": "bg-destructive-foreground/15",
  done: "bg-[color-mix(in_srgb,var(--viz-3)_22%,transparent)] text-[var(--viz-3)]",
};

/** The louder the state, the more times the badge calls out, then it stops. */
const PULSES: Partial<Record<BandTone, number>> = {
  due: 1,
  overdue: 2,
  "month-overdue": 3,
};

function rangeOf({ kind, anchor, part }: Selection): LockRange {
  if (part) return part;
  return kind === "month" ? monthRange(anchor) : weekRange(anchor);
}

function step({ kind, anchor }: Selection, direction: 1 | -1): string {
  const date = parseISO(anchor);
  return toIsoDate(
    kind === "month" ? addMonths(date, direction) : addWeeks(date, direction),
  );
}

function titleOf(selection: Selection) {
  if (selection.name) return selection.name;
  if (selection.part) return formatRange(selection.part);
  if (selection.kind === "month") {
    return format(parseISO(selection.anchor), "MMMM yyyy");
  }
  return `Week ${isoWeekOf(selection.anchor)}`;
}

/** "Lock week 40", but "Lock October 2026". */
function inSentence(scopeName: string) {
  return scopeName.replace(/^Week/, "week");
}

const shortDate = (date: string) => format(parseISO(date), "EEE d MMM");

/**
 * Follows what's due until the person browses elsewhere. A lock pins the
 * period so the rows can show it closing before moving on.
 */
function useSignOff() {
  const today = toIsoDate(new Date());
  const { days: dueDays, isLoaded, due, next } = useDueLock();
  const [picked, setPicked] = React.useState<Selection | null>(null);
  const [isJustLocked, setIsJustLocked] = React.useState(false);
  const { lock, unlock, isPending } = useLockActions();

  const selection: Selection =
    picked ??
    (due
      ? {
          kind: "due",
          anchor: due.to,
          part: { from: due.from, to: due.to },
          name: due.name,
        }
      : { kind: "week", anchor: today, part: null });
  const range = rangeOf(selection);

  const { days: rangeDays, isLoaded: isRangeLoaded } = useLockDays(range);
  const isInDueWindow =
    isLoaded &&
    range.from >= dueDays[0].date &&
    range.to <= dueDays[dueDays.length - 1].date;
  const days = isInDueWindow ? daysIn(dueDays, range) : rangeDays;

  const coversOneWeek = weekRange(range.from).from === weekRange(range.to).from;
  const parts =
    selection.kind === "week" ? splitAtMonthEnd(weekRange(selection.anchor)) : null;

  return {
    today,
    isLoaded,
    due,
    next,
    level: lockLevel(due, next?.dueOn, today),
    lockedThrough: lockedThrough(dueDays),
    selection,
    range,
    days,
    isReady: isInDueWindow || isRangeLoaded,
    summary: summarize(days),
    action: scopeAction(days),
    isFullyLocked:
      days.length > 0 && days.every((day) => isLocked(day) || day.isIdle),
    scopeName: titleOf(selection),
    coversOneWeek,
    parts,
    isPending,
    isJustLocked,
    isBrowsing: picked !== null && !isJustLocked,
    pickKind: (kind: "week" | "month") =>
      setPicked({ kind, anchor: range.from, part: null }),
    pickWeek: (anchor: string) =>
      setPicked({ kind: "week", anchor, part: null }),
    pickPart: (part: LockRange | null) =>
      setPicked({ kind: "week", anchor: range.from, part }),
    move: (direction: 1 | -1) =>
      setPicked({
        kind: selection.kind === "due" ? "week" : selection.kind,
        anchor: step(selection, direction),
        part: null,
      }),
    reset: () => setPicked(null),
    lockIt: async (onSettled?: () => void) => {
      setPicked(selection);
      try {
        await lock(range, rangesWithStatus(days, "open"));
      } catch {
        // The failure has its toast, and the period stays as it was.
        setPicked(null);
        return;
      }
      setIsJustLocked(true);
      setTimeout(() => {
        onSettled?.();
        setIsJustLocked(false);
        setPicked(null);
      }, SETTLE_MS);
    },
    unlockIt: () => void unlock(rangesWithStatus(days, "approved")),
  };
}

type Review = ReturnType<typeof useSignOff>;

/**
 * Sits beside the entries, so the review stays open while you fix what it
 * flags. It is one line until a lock is due, then it asks louder the later it gets.
 */
export function SignOffDock({ onShowDay }: { onShowDay: ShowDay }) {
  const review = useSignOff();
  const [isOpenedEarly, setIsOpenedEarly] = React.useState(false);

  if (!review.isLoaded) return <div className="h-10 xl:mb-6" />;

  const showsCard =
    review.due !== null ||
    review.isBrowsing ||
    review.isJustLocked ||
    isOpenedEarly;
  const close = () => {
    setIsOpenedEarly(false);
    review.reset();
  };

  return (
    <div className="xl:sticky xl:top-4 xl:z-10 xl:mb-6">
      <AnimatePresence initial={false} mode="wait">
        {showsCard ? (
          <motion.section
            key="card"
            aria-label="Sign off"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.25, ease: "easeOut" }}
            className="card-elevated overflow-hidden rounded-2xl"
          >
            <ReviewCard
              review={review}
              onClose={close}
              onShowDay={onShowDay}
              onLocked={() => setIsOpenedEarly(false)}
            />
          </motion.section>
        ) : (
          <motion.div
            key="line"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
          >
            <QuietLine review={review} onOpen={() => setIsOpenedEarly(true)} />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/** Nothing to do, so nothing to read: one line, and it never moves. */
function QuietLine(props: { review: Review; onOpen: () => void }) {
  const { review } = props;
  const isSoon = review.level === "soon";

  return (
    <button
      type="button"
      aria-label="Lock another period"
      onClick={props.onOpen}
      className="group flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-left text-sm text-muted-foreground outline-hidden transition-colors hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring"
    >
      {isSoon ? (
        <span className="flex h-4 w-4 shrink-0 items-center justify-center">
          <span className="h-2 w-2 rounded-full bg-primary" />
        </span>
      ) : (
        <LockGlyph closed className="h-4 w-4 shrink-0" />
      )}
      <span
        className={cn("min-w-0 flex-1 truncate", isSoon && "text-foreground")}
      >
        {isSoon && review.next
          ? `${capitalize(review.next.label)} due tomorrow`
          : review.lockedThrough
            ? `Locked through ${shortDate(review.lockedThrough)}`
            : "Time open"}
      </span>
      {!isSoon && review.next && (
        <span className="shrink-0 text-xs">
          Next {shortDate(review.next.dueOn)}
        </span>
      )}
      <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100" />
    </button>
  );
}

function bandTone(review: Review): BandTone {
  if (review.isJustLocked) return "done";
  if (review.isBrowsing || !review.due) return "neutral";
  return review.due.severity;
}

function ReviewCard(props: {
  review: Review;
  onClose: () => void;
  onShowDay: ShowDay;
  onLocked: () => void;
}) {
  const { review } = props;
  const tone = bandTone(review);
  const showsControls = tone === "neutral";
  const isClosed = review.isJustLocked || review.isFullyLocked;
  const title = review.isJustLocked
    ? `${capitalize(inSentence(review.scopeName))} locked`
    : tone === "neutral"
      ? capitalize(review.scopeName)
      : review.due?.title;
  const detail = review.isJustLocked
    ? "Nothing more to do here."
    : tone === "neutral"
      ? null
      : review.due?.detail;

  return (
    <>
      <div
        className={cn(
          "flex items-center gap-3 px-4 py-3 transition-colors duration-500",
          BAND_STYLES[tone],
        )}
      >
        <LockBadge tone={tone} closed={isClosed} />
        <div className="min-w-0 flex-1">
          <h3 className="font-display text-base font-semibold leading-tight">
            {title}
          </h3>
          {detail && <p className="text-sm opacity-80">{detail}</p>}
        </div>
        {tone === "neutral" && (
          <Button
            variant="ghost"
            size="icon"
            className="-mr-1 h-7 w-7 shrink-0"
            aria-label="Close"
            onClick={props.onClose}
          >
            <X className="h-4 w-4" />
          </Button>
        )}
      </div>

      <div className="px-4 pb-4 pt-2">
        {showsControls && <PeriodControls review={review} />}
        <div className="-mx-2 max-h-[50vh] overflow-y-auto py-1">
          <ReviewRows
            review={review}
            tone={tone}
            onShowDay={props.onShowDay}
          />
        </div>
        <ReviewFooter
          review={review}
          tone={tone}
          onLocked={props.onLocked}
        />
      </div>
    </>
  );
}

function LockBadge(props: { tone: BandTone; closed: boolean }) {
  const reduceMotion = useReducedMotion();
  const pulses = PULSES[props.tone] ?? 0;
  const isLate = props.tone === "overdue" || props.tone === "month-overdue";

  return (
    <span
      className={cn(
        "relative flex h-9 w-9 shrink-0 items-center justify-center rounded-full transition-colors duration-500",
        BADGE_STYLES[props.tone],
      )}
    >
      {!reduceMotion && pulses > 0 && (
        <motion.span
          key={props.tone}
          aria-hidden
          className="absolute inset-0 rounded-full border-2 border-current"
          initial={{ scale: 1, opacity: 0.7 }}
          animate={{ scale: 1.8, opacity: 0 }}
          transition={{
            delay: 0.4,
            duration: 1,
            repeat: pulses - 1,
            repeatDelay: 0.5,
          }}
        />
      )}
      {!reduceMotion && props.tone === "done" && (
        <motion.span
          aria-hidden
          className="absolute inset-0 rounded-full bg-current"
          initial={{ scale: 0.6, opacity: 0.4 }}
          animate={{ scale: 2, opacity: 0 }}
          transition={{ duration: 0.6, ease: "easeOut" }}
        />
      )}
      <motion.span
        className="flex"
        animate={
          isLate && !reduceMotion ? { rotate: [0, -10, 9, -6, 4, 0] } : undefined
        }
        transition={{ delay: 0.5, duration: 0.6 }}
      >
        <LockGlyph closed={props.closed} className="h-5 w-5" />
      </motion.span>
    </span>
  );
}

function PeriodControls({ review }: { review: Review }) {
  return (
    <div className="mb-1 flex flex-wrap items-center gap-x-3 gap-y-2 text-sm">
      {(["week", "month"] as const).map((kind) => (
        <button
          key={kind}
          type="button"
          aria-pressed={review.selection.kind === kind}
          onClick={() => review.pickKind(kind)}
          className={cn(
            "capitalize underline-offset-4 transition-colors",
            review.selection.kind === kind
              ? "font-medium underline decoration-primary decoration-2"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {kind}
        </button>
      ))}
      <span className="flex">
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          aria-label="Previous period"
          onClick={() => review.move(-1)}
        >
          <ChevronLeft className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          aria-label="Next period"
          onClick={() => review.move(1)}
        >
          <ChevronRight className="h-4 w-4" />
        </Button>
      </span>
      {review.parts?.map((part) => {
        const isActive =
          review.selection.part?.from === part.from &&
          review.selection.part.to === part.to;
        return (
          <button
            key={part.from}
            type="button"
            aria-pressed={isActive}
            onClick={() => review.pickPart(isActive ? null : part)}
            className={cn(
              "rounded-md px-2 py-0.5 text-xs transition-colors",
              isActive
                ? "bg-primary/15 text-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
          >
            {formatRange(part)}
          </button>
        );
      })}
    </div>
  );
}

function ReviewRows(props: {
  review: Review;
  tone: BandTone;
  onShowDay: ShowDay;
}) {
  const { review } = props;
  // Weekends and holidays with nothing on them have nothing to review.
  const days = review.days.filter((day) => !day.isIdle);

  return (
    <div className={cn(!review.isReady && "opacity-40")}>
      {review.coversOneWeek ? (
        days.map((day, index) => (
          <DayRow
            key={day.date}
            day={day}
            index={index}
            growsIn={props.tone !== "neutral" && props.tone !== "done"}
            onShow={() => props.onShowDay(day.date)}
          />
        ))
      ) : (
        <WeekRows days={review.days} onOpenWeek={review.pickWeek} />
      )}
    </div>
  );
}

function ReviewFooter(props: {
  review: Review;
  tone: BandTone;
  onLocked: () => void;
}) {
  const { review, tone } = props;
  const { state: timerState } = useTimeTrackingTimer();
  const locksRunningTimer =
    timerState === "running" &&
    review.action === "lock" &&
    review.today >= review.range.from &&
    review.today <= review.range.to;
  const difference =
    review.summary.reportedHours - review.summary.scheduledHours;
  const name = inSentence(review.scopeName);

  if (review.isJustLocked) return null;

  return (
    <div className="mt-2 flex flex-col gap-2.5 border-t border-border/50 pt-3">
      <p className="flex items-baseline justify-between text-sm">
        <span className="text-muted-foreground">
          <span className="time-display text-foreground">
            {formatHoursMinutes(review.summary.reportedHours)}
          </span>{" "}
          of{" "}
          <span className="time-display">
            {formatHoursMinutes(review.summary.scheduledHours)}
          </span>
        </span>
        <span
          className={cn(
            "time-display font-medium",
            difference < -0.2
              ? "text-primary"
              : difference > 0.2
                ? "text-[var(--viz-3)]"
                : "text-muted-foreground",
          )}
        >
          {difference > 0 ? "+" : ""}
          {formatHoursMinutes(difference)}
        </span>
      </p>

      {locksRunningTimer && (
        <p className="text-xs text-primary">
          Your timer is running. Save it first: a locked day can't take new
          entries.
        </p>
      )}

      {review.action === "lock" ? (
        <Button
          variant={tone === "month-overdue" ? "destructive" : "default"}
          className="w-full"
          disabled={review.isPending || !review.isReady}
          onClick={() => void review.lockIt(props.onLocked)}
        >
          <LockIcon className="mr-2 h-4 w-4" />
          {review.isPending ? "Locking..." : `Lock ${name}`}
        </Button>
      ) : review.action === "unlock" ? (
        <Button
          variant="ghost"
          size="sm"
          className="w-full text-muted-foreground"
          disabled={review.isPending || !review.isReady}
          onClick={review.unlockIt}
        >
          <LockOpenIcon className="mr-2 h-3.5 w-3.5" />
          Unlock {name}
        </Button>
      ) : (
        <p className="text-center text-sm text-muted-foreground">
          {review.isFullyLocked
            ? "Locked, and it can't be unlocked here."
            : "Nothing to lock in this period."}
        </p>
      )}

      {tone !== "neutral" && (
        <button
          type="button"
          onClick={() => review.pickKind("week")}
          className="self-center text-xs text-muted-foreground underline-offset-4 transition-colors hover:text-foreground hover:underline"
        >
          Lock another period
        </button>
      )}
    </div>
  );
}

/** Colour says how the day went; the hatched gap is the time that's missing. */
function DayBar(props: { day: LockDay; index: number; growsIn: boolean }) {
  const { day } = props;
  const reduceMotion = useReducedMotion();
  const scale = Math.max(day.scheduledHours, 8) * 1.25;
  const fill = Math.min(day.reportedHours / scale, 1) * 100;
  const mark = (day.scheduledHours / scale) * 100;
  const short = hoursShort(day);
  const delay = props.index * 0.05;

  return (
    <span className="relative block h-2 min-w-0 flex-1 rounded-full bg-muted">
      {short > 0 && (
        <span
          className="absolute inset-y-0 rounded-r-full bg-[repeating-linear-gradient(135deg,hsl(var(--primary)/0.6)_0_2px,transparent_2px_4px)]"
          style={{ left: `${fill}%`, width: `${mark - fill}%` }}
        />
      )}
      <motion.span
        className={cn(
          "absolute inset-y-0 left-0 rounded-full transition-colors duration-300 motion-reduce:transition-none",
          isLocked(day)
            ? "bg-foreground/50"
            : day.isToday
              ? "bg-primary/50"
              : short > 0
                ? "bg-primary"
                : "bg-[var(--viz-3)]",
        )}
        initial={
          props.growsIn && !reduceMotion
            ? { width: 0 }
            : { width: `${fill}%` }
        }
        animate={{ width: `${fill}%` }}
        transition={{ delay, duration: 0.45, ease: "easeOut" }}
        style={{ transitionDelay: `${props.index * 70}ms` }}
      />
      {day.scheduledHours > 0 && (
        <span
          className="absolute -inset-y-0.5 w-px bg-foreground/40"
          style={{ left: `${mark}%` }}
        />
      )}
    </span>
  );
}

function DayRow(props: {
  day: LockDay;
  index: number;
  growsIn: boolean;
  onShow: () => void;
}) {
  const { day } = props;
  const issues = dayIssues(day);
  const short = hoursShort(day);

  return (
    <button
      type="button"
      title={
        issues.length > 0
          ? issues.map((issue) => issue.text).join(", ")
          : "Show this day"
      }
      onClick={props.onShow}
      className="flex w-full items-center gap-2.5 rounded-md px-2 py-1 text-left text-sm outline-hidden transition-colors hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span
        className={cn(
          "w-14 shrink-0",
          day.isToday && "font-semibold",
          isLocked(day) && "text-muted-foreground",
        )}
      >
        {format(parseISO(day.date), "EEE d")}
      </span>
      <DayBar day={day} index={props.index} growsIn={props.growsIn} />
      <span className="flex w-12 shrink-0 items-center justify-end text-xs">
        {isLocked(day) ? (
          <LockIcon className="h-3 w-3 text-muted-foreground" />
        ) : short > 0 ? (
          <span className="time-display text-primary">
            -{formatHoursMinutes(short)}
          </span>
        ) : day.hasOverlap ? (
          <AlertTriangle className="h-3.5 w-3.5 text-primary" />
        ) : day.isToday ? (
          <span className="text-muted-foreground">Today</span>
        ) : null}
      </span>
      <span
        className={cn(
          "time-display w-11 shrink-0 text-right",
          isLocked(day) && "text-muted-foreground",
        )}
      >
        {formatHoursMinutes(day.reportedHours)}
      </span>
    </button>
  );
}

function WeekRows(props: {
  days: Array<LockDay>;
  onOpenWeek: (anchor: string) => void;
}) {
  const weekStarts = [
    ...new Set(props.days.map((day) => weekRange(day.date).from)),
  ];

  return weekStarts.map((weekStart) => {
    const inRange = daysIn(props.days, weekRange(weekStart));
    const summary = summarize(inRange);
    const first = inRange[0];
    const isDone = inRange.every((day) => isLocked(day) || day.isIdle);

    return (
      <button
        key={weekStart}
        type="button"
        onClick={() => props.onOpenWeek(first.date)}
        className="flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-muted/60"
      >
        <span className="w-16 shrink-0">Week {isoWeekOf(first.date)}</span>
        <span className="flex-1 text-xs text-muted-foreground">
          {formatRange({
            from: first.date,
            to: inRange[inRange.length - 1].date,
          })}
        </span>
        {isDone ? (
          <LockIcon className="h-3 w-3 text-muted-foreground" />
        ) : (
          summary.issueDays > 0 && (
            <span className="text-xs text-primary">
              {summary.issueDays} to check
            </span>
          )
        )}
        <span className="time-display w-12 shrink-0 text-right">
          {formatHoursMinutes(summary.reportedHours)}
        </span>
      </button>
    );
  });
}
