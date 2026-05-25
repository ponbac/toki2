import React from "react";
import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import {
  Baby,
  CalendarIcon,
  CalendarPlus,
  Clock,
  Loader2,
  MessageSquare,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import type { ManagedAbsenceType } from "@/lib/api/queries/time-tracking";
import type { CreateAbsencesPayload } from "@/lib/api/mutations/time-tracking";
import { absenceTypeIcons } from "./absence-types";

const childRequiredTypes = new Set<ManagedAbsenceType>([
  "parentalLeave",
  "childcare",
]);

function getTodayDate() {
  return dayjs().format("YYYY-MM-DD");
}

export function ReportAbsenceDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (payload: CreateAbsencesPayload) => void;
  isCreating?: boolean;
}) {
  const [from, setFrom] = React.useState(getTodayDate);
  const [to, setTo] = React.useState(getTodayDate);
  const [fromOpen, setFromOpen] = React.useState(false);
  const [toOpen, setToOpen] = React.useState(false);
  const [absenceType, setAbsenceType] =
    React.useState<ManagedAbsenceType | null>(null);
  const [submitAttempted, setSubmitAttempted] = React.useState(false);
  const [selectedChildName, setSelectedChildName] = React.useState("");
  const [comment, setComment] = React.useState("");
  const [hoursByDate, setHoursByDate] = React.useState<Record<string, string>>(
    {},
  );

  const dateRangeIsValid = !dayjs(from).isAfter(dayjs(to));

  const { data: availableAbsenceTypes, isLoading: isAbsenceTypesLoading } =
    useQuery({
      ...timeTrackingQueries.absenceTypes(),
      enabled: props.open,
    });
  const {
    data: absenceChildren = [],
    isError: isAbsenceChildrenError,
    isLoading: isAbsenceChildrenLoading,
  } = useQuery({
    ...timeTrackingQueries.absenceChildren(),
    enabled: props.open,
  });
  const { data: defaults, isLoading: isDefaultsLoading } = useQuery({
    ...timeTrackingQueries.absenceDayDefaults({ from, to }),
    enabled: props.open && dateRangeIsValid,
  });

  React.useEffect(() => {
    if (!availableAbsenceTypes || absenceType === null) return;
    if (
      !availableAbsenceTypes.some(
        (option) => option.absenceType === absenceType,
      )
    ) {
      setAbsenceType(null);
    }
  }, [availableAbsenceTypes, absenceType]);

  React.useEffect(() => {
    if (!props.open || !defaults) return;

    setHoursByDate((current) => {
      const next: Record<string, string> = {};
      defaults.forEach((day) => {
        next[day.date] = current[day.date] ?? String(day.scheduledHours);
      });
      return next;
    });
  }, [defaults, props.open]);

  const childOptions = React.useMemo(
    () => {
      const seen = new Set<string>();
      return absenceChildren.filter((child) => {
        if (seen.has(child.name)) return false;
        seen.add(child.name);
        return true;
      });
    },
    [absenceChildren],
  );
  const selectedChild =
    childOptions.find((child) => child.name === selectedChildName) ?? null;

  const { positiveDays, totalHours } = React.useMemo(() => {
    const days: CreateAbsencesPayload["days"] = [];
    let hours = 0;

    for (const day of defaults ?? []) {
      const dayHours = Number(hoursByDate[day.date] ?? day.scheduledHours);
      if (!Number.isFinite(dayHours) || dayHours <= 0) continue;

      days.push({
        date: day.date,
        hours: dayHours,
      });
      hours += dayHours;
    }

    return { positiveDays: days, totalHours: hours };
  }, [defaults, hoursByDate]);

  const childRequired =
    absenceType !== null && childRequiredTypes.has(absenceType);
  const childSelectionReady =
    !childRequired ||
    (!isAbsenceChildrenError &&
      !isAbsenceChildrenLoading &&
      selectedChild !== null);
  const canAttemptSubmit =
    dateRangeIsValid &&
    positiveDays.length > 0 &&
    Boolean(availableAbsenceTypes?.length) &&
    !isAbsenceTypesLoading &&
    !isDefaultsLoading &&
    !props.isCreating &&
    childSelectionReady;
  const canSubmit = Boolean(absenceType) && canAttemptSubmit;

  const SelectedTypeIcon = absenceType ? absenceTypeIcons[absenceType] : null;
  const showAbsenceTypeError = submitAttempted && absenceType === null;
  const showChildError =
    submitAttempted && childRequired && selectedChild === null;
  const submitDisabled = !canAttemptSubmit;

  React.useEffect(() => {
    if (!childRequired) {
      setSelectedChildName("");
      return;
    }

    if (
      selectedChildName &&
      !childOptions.some((child) => child.name === selectedChildName)
    ) {
      setSelectedChildName("");
    }
  }, [childOptions, childRequired, selectedChildName]);

  const reset = () => {
    const today = getTodayDate();
    setFrom(today);
    setTo(today);
    setAbsenceType(null);
    setSubmitAttempted(false);
    setSelectedChildName("");
    setComment("");
    setHoursByDate({});
  };

  const setHoursForDate = (date: string, hours: string) => {
    setHoursByDate((current) => ({
      ...current,
      [date]: hours,
    }));
  };

  const setFromDate = (value: string) => {
    const nextFrom = dayjs(value);
    setFrom(value);
    if (nextFrom.isAfter(dayjs(to))) {
      setTo(value);
    }
  };

  const setToDate = (value: string) => {
    const nextTo = dayjs(value);
    setTo(value);
    if (nextTo.isBefore(dayjs(from))) {
      setFrom(value);
    }
  };

  const submitAbsences = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitAttempted(true);
    if (!absenceType || !canSubmit) return;

    props.onCreate({
      absenceType,
      child: childRequired ? selectedChild?.name ?? null : null,
      comment,
      days: positiveDays,
    });
  };

  const handleOpenChange = (open: boolean) => {
    props.onOpenChange(open);
    if (!open) reset();
  };

  return (
    <Dialog open={props.open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-2xl">
        <form className="space-y-5" onSubmit={submitAbsences}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2.5">
              {SelectedTypeIcon ? (
                <SelectedTypeIcon className="h-5 w-5 text-primary" />
              ) : (
                <CalendarIcon className="h-5 w-5 text-muted-foreground" />
              )}
              Report Absence
            </DialogTitle>
            <DialogDescription>
              Create one absence entry for each selected day.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-3 sm:grid-cols-2">
            <DatePickerButton
              label="From"
              value={from}
              open={fromOpen}
              onOpenChange={setFromOpen}
              onChange={setFromDate}
            />
            <DatePickerButton
              label="To"
              value={to}
              open={toOpen}
              onOpenChange={setToOpen}
              onChange={setToDate}
            />
          </div>

          <div>
            <div className="space-y-1.5">
              <label
                htmlFor="absence-type"
                className="text-sm font-medium text-muted-foreground"
              >
                Absence type <span className="text-destructive">*</span>
              </label>
              <Select
                value={absenceType ?? undefined}
                disabled={
                  isAbsenceTypesLoading || availableAbsenceTypes?.length === 0
                }
                onValueChange={(value) => {
                  setAbsenceType(value as ManagedAbsenceType);
                  setSubmitAttempted(false);
                }}
              >
                <SelectTrigger
                  id="absence-type"
                  aria-invalid={showAbsenceTypeError}
                  aria-describedby={
                    showAbsenceTypeError ? "absence-type-error" : undefined
                  }
                  className={cn(
                    "data-[placeholder]:text-foreground/70",
                    showAbsenceTypeError &&
                      "border-destructive focus:ring-destructive",
                  )}
                >
                  <SelectValue placeholder="Select absence type..." />
                </SelectTrigger>
                <SelectContent>
                  {availableAbsenceTypes?.map((type) => {
                    const Icon = absenceTypeIcons[type.absenceType];
                    return (
                      <SelectItem
                        key={type.absenceType}
                        value={type.absenceType}
                        className="pl-2"
                      >
                        <span className="flex items-center gap-2">
                          <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
                          {type.absenceTypeLabel}
                        </span>
                      </SelectItem>
                    );
                  })}
                </SelectContent>
              </Select>
              {!isAbsenceTypesLoading &&
                availableAbsenceTypes?.length === 0 && (
                  <p className="text-sm text-muted-foreground">
                    No absence types are available in Kleer.
                  </p>
                )}
              {showAbsenceTypeError && (
                <p id="absence-type-error" className="text-sm text-destructive">
                  Choose an absence type to continue.
                </p>
              )}
            </div>

            <div
              className={cn(
                "grid transition-all duration-200 ease-out",
                childRequired
                  ? "mt-5 grid-rows-[1fr] opacity-100"
                  : "mt-0 grid-rows-[0fr] opacity-0",
              )}
            >
              <div className="overflow-hidden p-0.5">
                <div className="relative">
                  <Baby className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Select
                    value={selectedChildName}
                    disabled={
                      !childRequired ||
                      isAbsenceChildrenError ||
                      isAbsenceChildrenLoading ||
                      absenceChildren.length === 0
                    }
                    onValueChange={(value) => {
                      setSelectedChildName(value);
                      setSubmitAttempted(false);
                    }}
                  >
                    <SelectTrigger
                      aria-invalid={showChildError}
                      aria-describedby={
                        showChildError ? "absence-child-error" : undefined
                      }
                      className={cn(
                        "pl-9 data-[placeholder]:text-foreground/70",
                        showChildError &&
                          "border-destructive focus:ring-destructive",
                      )}
                      tabIndex={childRequired ? 0 : -1}
                    >
                      <SelectValue
                        placeholder={
                          isAbsenceChildrenLoading
                            ? "Loading children..."
                            : isAbsenceChildrenError
                              ? "Could not load children"
                            : "Select child..."
                        }
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {childOptions.map((child) => (
                        <SelectItem
                          key={`${child.name}:${child.birthDate ?? ""}`}
                          value={child.name}
                        >
                          {child.birthDate
                            ? `${child.name} - ${dayjs(child.birthDate).format("YYYY-MM-DD")}`
                            : child.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                {childRequired && isAbsenceChildrenError && (
                  <p className="mt-1.5 text-sm text-destructive">
                    Could not load registered children from Kleer.
                  </p>
                )}
                {childRequired &&
                  !isAbsenceChildrenError &&
                  !isAbsenceChildrenLoading &&
                  absenceChildren.length === 0 && (
                    <p className="mt-1.5 text-sm text-muted-foreground">
                      No registered children found in Kleer.
                    </p>
                  )}
                {showChildError && (
                  <p
                    id="absence-child-error"
                    className="mt-1.5 text-sm text-destructive"
                  >
                    Select a registered child to continue.
                  </p>
                )}
              </div>
            </div>
          </div>

          <div className="rounded-lg border border-border/60 bg-muted/20">
            <div className="flex items-center justify-between border-b border-border/60 px-4 py-2.5">
              <span className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                <Clock className="h-3.5 w-3.5" />
                Schedule
              </span>
              {!isDefaultsLoading && defaults && defaults.length > 0 && (
                <span className="font-mono text-xs text-muted-foreground">
                  Total:{" "}
                  <span className="font-semibold text-foreground">
                    {totalHours}h
                  </span>
                </span>
              )}
            </div>

            <ScrollArea className="h-[min(16rem,_calc(100vh-28rem))]">
              {isDefaultsLoading ? (
                <div className="flex items-center justify-center gap-2 px-4 py-8 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Loading schedule...
                </div>
              ) : defaults?.length === 0 ? (
                <div className="px-4 py-8 text-center text-sm text-muted-foreground">
                  No days in selected range.
                </div>
              ) : (
                <div className="divide-y divide-border/40">
                  {defaults?.map((day) => {
                    const dayOfWeek = dayjs(day.date).day();
                    const isWeekend = dayOfWeek === 0 || dayOfWeek === 6;
                    const isZero =
                      Number(hoursByDate[day.date] ?? day.scheduledHours) === 0;

                    return (
                      <div
                        key={day.date}
                        className={cn(
                          "group grid grid-cols-[1fr_5rem_5rem] items-center gap-3 px-4 py-2.5 transition-colors hover:bg-muted/30",
                          isZero && "opacity-50",
                        )}
                      >
                        <span
                          className={cn(
                            "text-sm",
                            isWeekend && "text-muted-foreground",
                          )}
                        >
                          {dayjs(day.date).format("ddd, MMM D")}
                          {isWeekend && (
                            <span className="ml-1.5 text-[10px] uppercase tracking-wider text-muted-foreground/60">
                              off
                            </span>
                          )}
                        </span>
                        <span className="text-right font-mono text-sm text-muted-foreground">
                          {day.scheduledHours}h
                        </span>
                        <Input
                          type="number"
                          min="0"
                          max="24"
                          step="0.25"
                          value={
                            hoursByDate[day.date] ?? String(day.scheduledHours)
                          }
                          onChange={(event) =>
                            setHoursForDate(day.date, event.target.value)
                          }
                          className="h-8 text-center font-mono"
                        />
                      </div>
                    );
                  })}
                </div>
              )}
            </ScrollArea>
          </div>

          <div className="relative">
            <MessageSquare className="pointer-events-none absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
            <Textarea
              value={comment}
              onChange={(event) => setComment(event.target.value)}
              placeholder="Comment (optional)"
              className="min-h-[4.5rem] pl-9 pt-2.5"
            />
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={props.isCreating}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={submitDisabled}>
              {props.isCreating ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Reporting...
                </>
              ) : (
                <>
                  <CalendarPlus className="mr-2 h-4 w-4" />
                  Report Absence
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function DatePickerButton({
  label,
  value,
  open,
  onOpenChange,
  onChange,
}: {
  label: string;
  value: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChange: (value: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-sm font-medium text-muted-foreground">
        {label}
      </label>
      <Popover open={open} onOpenChange={onOpenChange}>
        <PopoverTrigger asChild>
          <Button variant="outline" className="w-full justify-start">
            <CalendarIcon className="mr-2 h-4 w-4" />
            {dayjs(value).format("ddd, MMM D, YYYY")}
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-auto p-0" align="start">
          <Calendar
            mode="single"
            selected={dayjs(value).toDate()}
            onSelect={(date) => {
              if (!date) return;
              onChange(dayjs(date).format("YYYY-MM-DD"));
              onOpenChange(false);
            }}
            weekStartsOn={1}
            initialFocus
          />
        </PopoverContent>
      </Popover>
    </div>
  );
}
