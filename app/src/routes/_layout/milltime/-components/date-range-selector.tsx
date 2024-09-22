import { DateRange } from "react-day-picker";
import {
  format,
  parseISO,
  startOfWeek,
  endOfWeek,
  startOfMonth,
  startOfYear,
  subDays,
} from "date-fns";
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

  const thisWeekSelected = rangeIsEqual(dateRange, thisWeekRange);

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
          <PresetRangeButtons
            currentDateRange={dateRange}
            setDateRange={props.setDateRange}
          />
          <Calendar
            initialFocus
            mode="range"
            defaultMonth={dateRange.from}
            selected={dateRange}
            onSelect={handleRangeSelect}
            numberOfMonths={2}
            weekStartsOn={1}
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

type PresetRange =
  | "This Week"
  | "This Month"
  | "This Year"
  | "Last 30 Days"
  | "Last 365 Days";

function PresetRangeButtons(props: {
  currentDateRange: { from: Date; to: Date };
  setDateRange: (dateRange: { from: string; to: string }) => void;
}) {
  const today = new Date();
  const ranges: Record<PresetRange, { from: Date; to: Date }> = {
    "This Week": {
      from: startOfWeek(today, { weekStartsOn: 1 }),
      to: today,
    },
    "This Month": {
      from: startOfMonth(today),
      to: today,
    },
    "This Year": {
      from: startOfYear(today),
      to: today,
    },
    "Last 30 Days": {
      from: subDays(today, 30),
      to: today,
    },
    "Last 365 Days": {
      from: subDays(today, 365),
      to: today,
    },
  };

  const selectedPreset = Object.keys(ranges).find((key) =>
    rangeIsEqual(props.currentDateRange, ranges[key as PresetRange]),
  ) as PresetRange | undefined;

  const handlePresetRangeClick = (preset: PresetRange) => {
    const range = ranges[preset];
    props.setDateRange({
      from: format(range.from, "yyyy-MM-dd"),
      to: format(range.to, "yyyy-MM-dd"),
    });
  };

  return (
    <div className="flex flex-row justify-center gap-4 px-2 pt-2">
      {Object.keys(ranges).map((preset) => (
        <PresetRangeButton
          key={preset}
          preset={preset as PresetRange}
          selected={selectedPreset === preset}
          onClick={() => handlePresetRangeClick(preset as PresetRange)}
        />
      ))}
    </div>
  );
}

function PresetRangeButton(props: {
  preset: PresetRange;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <Button
      size="sm"
      variant="outline"
      className="text-xs"
      onClick={props.onClick}
      disabled={props.selected}
    >
      {props.preset}
    </Button>
  );
}

function rangeIsEqual(
  range1: { from: Date; to: Date },
  range2: { from: Date; to: Date },
) {
  return (
    format(range1.from, "yyyy-MM-dd") === format(range2.from, "yyyy-MM-dd") &&
    format(range1.to, "yyyy-MM-dd") === format(range2.to, "yyyy-MM-dd")
  );
}
