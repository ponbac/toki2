import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatHoursAsHoursMinutes } from "@/lib/utils";
import { ChevronRight, CalendarX2, Trash2 } from "lucide-react";
import { formatAbsenceDetails } from "../-helpers/absence-display";
import type { AbsenceListEntry } from "../-helpers/time-entry-list-types";
import { absenceTypeAccent, absenceTypeIcons } from "./absence-types";
import { EntryCardFrame } from "./entry-card-frame";

export function AbsenceEntryCard(props: {
  entry: AbsenceListEntry;
  onDelete: () => void;
}) {
  const absence = props.entry.absence;
  const Icon = absenceTypeIcons[absence.absenceType] ?? CalendarX2;
  const details = formatAbsenceDetails(absence);

  return (
    <EntryCardFrame className={absenceTypeAccent[absence.absenceType]}>
      <div className="flex flex-col items-center">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-background/80 text-muted-foreground">
          <Icon className="h-4 w-4" />
        </div>
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <h3 className="truncate font-semibold leading-tight">
              {absence.absenceTypeLabel}
            </h3>
            <p className="text-sm text-muted-foreground">
              {details ?? "Scheduled absence"}
            </p>
          </div>

          <div className="shrink-0">
            <div className="rounded-lg bg-background/80 px-3 py-1.5 text-sm font-semibold transition-opacity group-hover:opacity-0">
              <span className="time-display">
                {formatHoursAsHoursMinutes(absence.hours)}
              </span>
            </div>
            {absence.deletable && (
              <div className="absolute right-4 top-4 flex items-center gap-1 rounded-lg border border-border/50 bg-card p-1 opacity-0 shadow-sm transition-opacity group-hover:opacity-100">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={props.onDelete}
                      className="h-7 w-7 rounded-md p-0 hover:bg-destructive/10 hover:text-destructive"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Delete absence</TooltipContent>
                </Tooltip>
              </div>
            )}
          </div>
        </div>

        <div className="mt-2 flex items-center gap-1.5 text-sm text-muted-foreground">
          <span className="time-display">{props.entry.startLabel}</span>
          <ChevronRight className="h-3 w-3" />
          <span className="time-display">{props.entry.endLabel}</span>
          {props.entry.isCapped && (
            <span className="text-xs text-muted-foreground/70">
              capped at day end
            </span>
          )}
        </div>
      </div>
    </EntryCardFrame>
  );
}
