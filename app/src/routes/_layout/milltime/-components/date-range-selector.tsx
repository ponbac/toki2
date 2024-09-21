import { DateRange } from "react-day-picker";
import { format, parseISO, startOfWeek, endOfWeek } from "date-fns";
import { CalendarIcon, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import { Calendar } from "@/components/ui/calendar";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export function DateRangeSelector(props: {
  dateRange: { from: string; to: string };
  setDateRange: (dateRange: { from: string; to: string }) => void;
}) {
  const dateRange = {
    from: parseISO(props.dateRange.from),
    to: parseISO(props.dateRange.to),
  };
  const thisWeekRange = {
    from: startOfWeek(new Date(), { weekStartsOn: 1 }),
    to: endOfWeek(new Date(), { weekStartsOn: 1 }),
  };

  const handleRangeSelect = (range: DateRange | undefined) => {
    if (range && range.from && range.to) {
      props.setDateRange({
        from: format(range.from, "yyyy-MM-dd"),
        to: format(range.to, "yyyy-MM-dd"),
      });
    }
  };

  const thisWeekSelected =
    format(dateRange.from, "yyyy-MM-dd") ===
      format(thisWeekRange.from, "yyyy-MM-dd") &&
    format(dateRange.to, "yyyy-MM-dd") ===
      format(thisWeekRange.to, "yyyy-MM-dd");

  return (
    <div className="flex flex-row items-center gap-2">
      <Popover>
        <PopoverTrigger asChild>
          <Button
            id="date"
            variant="outline"
            className={cn(
              "w-[300px] justify-start text-left font-normal",
              !dateRange.from && "text-muted-foreground",
            )}
          >
            <CalendarIcon className="mr-2 h-4 w-4" />
            {dateRange.from ? (
              dateRange.to ? (
                <>
                  {format(dateRange.from, "LLL dd, y")} -{" "}
                  {format(dateRange.to, "LLL dd, y")}
                </>
              ) : (
                format(dateRange.from, "LLL dd, y")
              )
            ) : (
              <span>Pick a date</span>
            )}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            initialFocus
            mode="range"
            defaultMonth={dateRange.from}
            selected={dateRange}
            onSelect={handleRangeSelect}
            numberOfMonths={2}
          />
        </PopoverContent>
      </Popover>
      {!thisWeekSelected && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              onClick={() => handleRangeSelect(thisWeekRange)}
              variant="ghost"
              size="icon"
              aria-label="Reset date range"
              disabled={thisWeekSelected}
            >
              <RotateCcw className="h-4 w-4 disabled:text-red-500" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            <p className="text-sm">Reset date range to current week</p>
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  );
}
