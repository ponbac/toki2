import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { monthLabel, previousMonth, shiftMonth } from "../-lib/format";

/** The months offered in the picker: two years back to next month. */
function recentMonths(selected: string): string[] {
  const latest = shiftMonth(previousMonth(), 2);
  const months = Array.from({ length: 26 }, (_, index) =>
    shiftMonth(latest, -index),
  );
  return months.includes(selected) ? months : [selected, ...months];
}

/** Picks a calendar month, `YYYY-MM`. */
export function MonthPicker({
  month,
  onChange,
}: {
  month: string;
  onChange: (month: string) => void;
}) {
  return (
    <div className="flex items-center gap-1">
      <Button
        type="button"
        variant="outline"
        size="icon"
        aria-label="Previous month"
        onClick={() => onChange(shiftMonth(month, -1))}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <Select value={month} onValueChange={onChange}>
        <SelectTrigger className="w-44" aria-label="Month">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {recentMonths(month).map((option) => (
            <SelectItem key={option} value={option}>
              {monthLabel(option)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button
        type="button"
        variant="outline"
        size="icon"
        aria-label="Next month"
        onClick={() => onChange(shiftMonth(month, 1))}
      >
        <ChevronRight className="size-4" />
      </Button>
    </div>
  );
}
