import * as React from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
} from "lucide-react";
import { DayPicker } from "react-day-picker";

import { cn } from "@/lib/utils";
import { buttonVariants } from "@/components/ui/button";

/** DayPicker's selection, navigation, and customization props for the app calendar. */
export type CalendarProps = React.ComponentProps<typeof DayPicker>;

/** Renders DayPicker with the app's calendar styling and accessible navigation. */
function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  ...props
}: CalendarProps) {
  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      className={cn("p-3", className)}
      navLayout="around"
      classNames={{
        months: "relative flex flex-col gap-4 sm:flex-row",
        month: "relative flex flex-col gap-4",
        month_caption: "flex h-7 items-center justify-center",
        caption_label: "text-sm font-medium",
        nav: "absolute inset-x-1 top-0 flex items-center justify-between",
        button_previous: cn(
          buttonVariants({ variant: "outline" }),
          "absolute left-1 top-0 h-7 w-7 bg-transparent p-0 opacity-50 hover:opacity-100 aria-disabled:pointer-events-none aria-disabled:opacity-25",
        ),
        button_next: cn(
          buttonVariants({ variant: "outline" }),
          "absolute right-1 top-0 h-7 w-7 bg-transparent p-0 opacity-50 hover:opacity-100 aria-disabled:pointer-events-none aria-disabled:opacity-25",
        ),
        month_grid: "w-full border-collapse",
        weekdays: "flex",
        weekday:
          "text-muted-foreground rounded-md w-9 font-normal text-[0.8rem]",
        week: "flex w-full mt-2",
        day: "group/day h-9 w-9 text-center text-sm p-0 relative aria-selected:bg-accent first:aria-selected:rounded-l-md last:aria-selected:rounded-r-md focus-within:relative focus-within:z-20",
        day_button: cn(
          buttonVariants({ variant: "ghost" }),
          "h-9 w-9 p-0 font-normal group-aria-selected/day:opacity-100",
        ),
        range_start: "range-start aria-selected:rounded-l-md",
        range_end: "range-end aria-selected:rounded-r-md",
        selected:
          "[&:not(.range-middle,.day-outside)>button]:bg-primary [&:not(.range-middle,.day-outside)>button]:text-primary-foreground [&:not(.range-middle,.day-outside)>button:hover]:bg-primary [&:not(.range-middle,.day-outside)>button:hover]:text-primary-foreground [&:not(.range-middle,.day-outside)>button:focus]:bg-primary [&:not(.range-middle,.day-outside)>button:focus]:text-primary-foreground",
        today:
          "[&:not([aria-selected])>button]:bg-accent [&:not([aria-selected])>button]:text-accent-foreground",
        outside:
          "day-outside text-muted-foreground aria-selected:bg-accent/50 [&[aria-selected]>button]:bg-accent/50 [&[aria-selected]>button]:text-muted-foreground",
        disabled:
          "text-muted-foreground [&:not(:has(button))]:opacity-50 [&>button]:opacity-50",
        range_middle:
          "range-middle [&>button]:rounded-none [&>button]:bg-accent [&>button]:text-accent-foreground [&>button:hover]:bg-accent [&>button:hover]:text-accent-foreground",
        hidden: "invisible",
        ...classNames,
      }}
      components={{
        Chevron: ({ orientation, className, size }) => {
          const Icon =
            orientation === "left"
              ? ChevronLeft
              : orientation === "right"
                ? ChevronRight
                : orientation === "up"
                  ? ChevronUp
                  : ChevronDown;

          return <Icon className={cn("h-4 w-4", className)} size={size} />;
        },
      }}
      {...props}
    />
  );
}
Calendar.displayName = "Calendar";

export { Calendar };
