import { useQuery } from "@tanstack/react-query";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { formatHoursMinutes } from "@/lib/utils";
import { endOfWeek, format, startOfWeek } from "date-fns";
import { Progress } from "@/components/ui/progress";
import { CalendarClockIcon, PiggyBankIcon } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export const TimeStats = () => {
  // Fetch time information
  const { data: timeInfo } = useQuery({
    ...milltimeQueries.timeInfo({
      from: format(startOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
      to: format(endOfWeek(new Date(), { weekStartsOn: 1 }), "yyyy-MM-dd"),
    }),
    staleTime: 5 * 60 * 1000,
  });

  const workedHours = timeInfo ? Math.floor(timeInfo.workedPeriodTime) : 0;
  const percentageCompleted = timeInfo
    ? (timeInfo.workedPeriodTime / timeInfo.scheduledPeriodTime) * 100
    : 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle>This Week</CardTitle>
        <CardDescription>
          You have worked {workedHours} {workedHours === 1 ? "hour" : "hours"}{" "}
          this week.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <div className="flex flex-row justify-around">
          <Tooltip>
            <TooltipTrigger className="cursor-default">
              <div className="flex items-center gap-2">
                <CalendarClockIcon size={26} />
                <div className="flex flex-col items-center justify-center">
                  <p className="text-lg">
                    {formatHoursMinutes(timeInfo?.periodTimeLeft ?? 0)}
                  </p>
                </div>
              </div>
            </TooltipTrigger>
            <TooltipContent>Hours left to work this week</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger className="cursor-default">
              <div className="flex items-center gap-2">
                <PiggyBankIcon size={26} />
                <div className="flex flex-col items-center justify-center">
                  <p className="text-lg">
                    {formatHoursMinutes(timeInfo?.flexTimeCurrent ?? 0)}
                  </p>
                </div>
              </div>
            </TooltipTrigger>
            <TooltipContent>Total flex time</TooltipContent>
          </Tooltip>
        </div>
        <div className="relative h-6 w-full">
          <Progress
            key={percentageCompleted}
            value={percentageCompleted}
            className="h-full w-full"
          />
          <div className="absolute inset-0 flex items-center justify-center">
            <span className="text-sm font-medium">
              {percentageCompleted.toFixed(1)}%
            </span>
          </div>
        </div>
      </CardContent>
    </Card>
  );
};
