import { useQuery } from "@tanstack/react-query";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { formatHoursMinutes } from "@/lib/utils";
import { endOfWeek, format, startOfWeek } from "date-fns";
import {
  CalendarClockIcon,
  PiggyBankIcon,
  TrendingUp,
  Sparkles,
} from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

const PROGRESS_GLOW_STYLE = {
  filter: "drop-shadow(0 0 6px hsl(var(--primary) / 0.4))",
} as const;

export const TimeStats = () => {
  const { data: timeInfo } = useQuery({
    ...timeTrackingQueries.timeInfo({
      from: format(startOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
      to: format(endOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
    }),
    staleTime: 5 * 60 * 1000,
  });

  const workedHours = timeInfo ? timeInfo.workedPeriodTime : 0;
  const scheduledHours = timeInfo ? timeInfo.scheduledPeriodTime : 40;
  const percentageCompleted =
    timeInfo && timeInfo.scheduledPeriodTime > 0
      ? (timeInfo.workedPeriodTime / timeInfo.scheduledPeriodTime) * 100
      : 0;
  const flexTime = timeInfo?.flexTimeCurrent ?? 0;
  const isAhead = percentageCompleted >= 100;

  return (
    <div className="card-elevated overflow-hidden rounded-2xl">
      {/* Header */}
      <div className="relative overflow-hidden border-b border-border/50 bg-gradient-to-br from-primary/10 via-primary/5 to-transparent px-5 py-4">
        <div className="absolute -right-8 -top-8 h-24 w-24 rounded-full bg-primary/10 blur-2xl" />
        <div className="relative flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/20">
            <TrendingUp className="h-5 w-5 text-primary" />
          </div>
          <div>
            <h3 className="font-display text-lg font-semibold">This Week</h3>
            <p className="text-sm text-muted-foreground">
              {formatHoursMinutes(workedHours)} of{" "}
              {formatHoursMinutes(scheduledHours)}
            </p>
          </div>
        </div>
      </div>

      {/* Progress section */}
      <div className="p-5">
        {/* Circular progress indicator */}
        <div className="mb-6 flex justify-center">
          <div className="relative">
            <svg className="h-32 w-32 -rotate-90 transform">
              {/* Background circle */}
              <circle
                cx="64"
                cy="64"
                r="56"
                fill="none"
                stroke="hsl(var(--muted))"
                strokeWidth="8"
              />
              {/* Progress circle */}
              <circle
                cx="64"
                cy="64"
                r="56"
                fill="none"
                stroke="hsl(var(--primary))"
                strokeWidth="8"
                strokeLinecap="round"
                strokeDasharray={`${(Math.min(percentageCompleted, 100) * 3.52).toFixed(2)} 352`}
                className="transition-all duration-1000 ease-out"
                style={PROGRESS_GLOW_STYLE}
              />
            </svg>
            <div className="absolute inset-0 flex flex-col items-center justify-center">
              <span className="font-display text-2xl font-bold">
                {percentageCompleted.toFixed(0)}%
              </span>
              {isAhead && (
                <Sparkles className="mt-1 h-4 w-4 animate-pulse-slow text-primary" />
              )}
            </div>
          </div>
        </div>

        {/* Stats grid */}
        <div className="grid grid-cols-2 gap-4">
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="group cursor-default rounded-xl bg-muted/30 p-4 transition-colors hover:bg-muted/50">
                <div className="mb-2 flex items-center gap-2 text-muted-foreground">
                  <CalendarClockIcon className="h-4 w-4" />
                  <span className="text-xs font-medium uppercase tracking-wider">
                    Remaining
                  </span>
                </div>
                <p className="time-display text-xl font-semibold">
                  {formatHoursMinutes(
                    Math.max(0, timeInfo?.periodTimeLeft ?? 0),
                  )}
                </p>
              </div>
            </TooltipTrigger>
            <TooltipContent>Hours left to work this week</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <div className="group cursor-default rounded-xl bg-muted/30 p-4 transition-colors hover:bg-muted/50">
                <div className="mb-2 flex items-center gap-2 text-muted-foreground">
                  <PiggyBankIcon className="h-4 w-4" />
                  <span className="text-xs font-medium uppercase tracking-wider">
                    Flex
                  </span>
                </div>
                <p
                  className={`time-display text-xl font-semibold ${
                    flexTime >= 0 ? "text-emerald-500" : "text-amber-500"
                  }`}
                >
                  {flexTime >= 0 ? "+" : ""}
                  {formatHoursMinutes(flexTime)}
                </p>
              </div>
            </TooltipTrigger>
            <TooltipContent>Total flex time</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </div>
  );
};
