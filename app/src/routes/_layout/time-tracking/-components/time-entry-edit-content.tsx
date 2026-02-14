import { useState } from "react";
import dayjs from "dayjs";
import { CalendarIcon, SaveIcon, TrashIcon } from "lucide-react";
import { toast } from "sonner";
import { TimeEntry } from "@/lib/api/queries/time-tracking";
import { cn, getWeekNumber } from "@/lib/utils";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { useTimeTrackingData } from "@/hooks/useTimeTrackingData";
import { Combobox } from "@/components/combobox";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Calendar } from "@/components/ui/calendar";

type TimeEntryEditContentProps = {
  entry: TimeEntry;
  onSaved: () => void;
  onCancel: () => void;
  variant: "inline" | "dialog";
};

export function TimeEntryEditContent(props: TimeEntryEditContentProps) {
  const [note, setNote] = useState(props.entry.note);
  const [hours, setHours] = useState(Math.floor(props.entry.hours));
  const [minutes, setMinutes] = useState(
    Math.round((props.entry.hours - Math.floor(props.entry.hours)) * 60),
  );
  const [selectedDate, setSelectedDate] = useState(props.entry.date);
  const [isDateOpen, setIsDateOpen] = useState(false);
  const [startTime, setStartTime] = useState(
    props.entry.startTime ? dayjs(props.entry.startTime).format("HH:mm") : "06:00",
  );
  const [endTime, setEndTime] = useState(() => {
    if (props.entry.endTime) return dayjs(props.entry.endTime).format("HH:mm");
    const initialStart = props.entry.startTime
      ? dayjs(props.entry.startTime).format("HH:mm")
      : "06:00";
    const initialHours = Math.floor(props.entry.hours);
    const initialMinutes = Math.round(
      (props.entry.hours - Math.floor(props.entry.hours)) * 60,
    );
    if (initialStart && (initialHours > 0 || initialMinutes > 0)) {
      const startDate = dayjs(`2000-01-01T${initialStart}`);
      return startDate
        .add(initialHours, "hour")
        .add(initialMinutes, "minute")
        .format("HH:mm");
    }
    return "";
  });

  const [projectId, setProjectId] = useState(props.entry.projectId);
  const [projectName, setProjectName] = useState(props.entry.projectName);
  const [activityId, setActivityId] = useState(props.entry.activityId);
  const [activityName, setActivityName] = useState(props.entry.activityName);

  const { projects, activities } = useTimeTrackingData({
    projectId,
    enabled: true,
  });

  const updateTimeRange = (start: string, end: string) => {
    setStartTime(start);
    setEndTime(end);
    if (start && end) {
      const startDate = dayjs(`2000-01-01T${start}`);
      const endDate = dayjs(`2000-01-01T${end}`);
      const diffHours = endDate.diff(startDate, "hour", true);
      setHours(Math.floor(diffHours));
      setMinutes(Math.round((diffHours - Math.floor(diffHours)) * 60));
    }
  };

  const updateTotalTime = (h: number, m: number) => {
    setHours(h);
    setMinutes(m);
    if (startTime) {
      const startDate = dayjs(`2000-01-01T${startTime}`);
      const endDate = startDate.add(h, "hour").add(m, "minute");
      setEndTime(endDate.format("HH:mm"));
    }
  };

  const { mutate: updateTimeEntry, isPending: isUpdatingTimeEntry } =
    timeTrackingMutations.useEditProjectRegistration({
      onSuccess: () => props.onSaved(),
      onError: () => toast.error(`Failed to update time entry, try again later`),
    });

  const { mutate: deleteTimeEntry, isPending: isDeletingTimeEntry } =
    timeTrackingMutations.useDeleteProjectRegistration({
      onSuccess: () => {
        props.onSaved();
        toast.success("Time entry deleted successfully");
      },
      onError: () => toast.error("Failed to delete time entry, try again later"),
    });

  const handleSave = () => {
    const startDateTime = dayjs(`${selectedDate}T${startTime}`);
    const computedEndTime = endTime
      ? endTime
      : startTime
        ? dayjs(`2000-01-01T${startTime}`)
            .add(hours, "hour")
            .add(minutes, "minute")
            .format("HH:mm")
        : "";
    const endDateTime = dayjs(`${selectedDate}T${computedEndTime}`);

    updateTimeEntry({
      projectRegistrationId: props.entry.registrationId,
      userNote: note ?? "",
      projectId,
      projectName,
      activityId,
      activityName,
      startTime: startDateTime.toISOString(),
      endTime: endDateTime.toISOString(),
      regDay: selectedDate,
      weekNumber: getWeekNumber(new Date(selectedDate)),
      originalRegDay: dayjs(props.entry.date).format("YYYY-MM-DD"),
      originalProjectId: props.entry.projectId,
      originalActivityId: props.entry.activityId,
    });
  };

  const handleDelete = () => {
    if (
      window.confirm(
        "Are you sure you want to delete this time entry? This action cannot be undone.",
      )
    ) {
      deleteTimeEntry({
        projectRegistrationId: props.entry.registrationId,
      });
    }
  };

  const handleProjectChange = (newProjectId: string) => {
    const selectedProject = projects?.find(
      (project) => project.projectId.toString() === newProjectId,
    );
    if (selectedProject) {
      setProjectId(newProjectId);
      setProjectName(selectedProject.projectName);
      setActivityId("");
      setActivityName("");
    }
  };

  const handleActivityChange = (newActivityId: string) => {
    const selectedActivity = activities?.find(
      (activity) => activity.activity === newActivityId,
    );
    if (selectedActivity) {
      setActivityId(selectedActivity.activity);
      setActivityName(selectedActivity.activityName);
    }
  };

  const body = (
    <>
      <div className={cn("space-y-5", props.variant === "inline" ? "p-5" : "pb-2")}>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2">
            <label className="text-sm font-medium">Project</label>
            <Combobox
              items={
                projects?.map((project) => ({
                  value: project.projectId.toString(),
                  label: project.projectName,
                })) || []
              }
              placeholder="Select project..."
              searchPlaceholder="Search projects..."
              onSelect={() => {}}
              emptyMessage="No projects found"
              value={projectId}
              onChange={handleProjectChange}
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium">Activity</label>
            <Combobox
              items={
                activities?.map((activity) => ({
                  value: activity.activity,
                  label: activity.activityName,
                })) || []
              }
              placeholder="Select activity..."
              searchPlaceholder="Search activities..."
              onSelect={() => {}}
              emptyMessage="No activities found"
              disabled={!projectId}
              value={activityId}
              onChange={handleActivityChange}
            />
          </div>
        </div>

        <div>
          <label className="mb-2 block text-sm font-medium">Date</label>
          <Popover open={isDateOpen} onOpenChange={setIsDateOpen}>
            <PopoverTrigger asChild>
              <Button
                variant="outline"
                className="w-full justify-start rounded-xl border-border/50 bg-muted/30 font-normal hover:bg-muted/50 sm:w-[240px]"
              >
                <CalendarIcon className="mr-2 h-4 w-4 text-muted-foreground" />
                {dayjs(selectedDate).format("ddd, MMM D, YYYY")}
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-auto p-0" align="start">
              <Calendar
                mode="single"
                selected={new Date(selectedDate)}
                onSelect={(date) => {
                  if (date) {
                    setSelectedDate(dayjs(date).format("YYYY-MM-DD"));
                    setIsDateOpen(false);
                  }
                }}
                weekStartsOn={1}
                initialFocus
              />
            </PopoverContent>
          </Popover>
        </div>

        <div className="space-y-2">
          <label className="text-sm font-medium">Note</label>
          <Input
            value={note ?? ""}
            onChange={(event) => setNote(event.target.value)}
            className="rounded-xl border-border/50 bg-muted/30"
            placeholder="What did you work on?"
          />
        </div>

        <div className="flex flex-wrap items-end gap-6">
          <div className="space-y-3">
            <h4 className="text-sm font-medium text-muted-foreground">Time Range</h4>
            <div className="flex gap-3">
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">Start</label>
                <Input
                  type="time"
                  value={startTime}
                  onChange={(event) => updateTimeRange(event.target.value, endTime)}
                  className="time-display w-28 rounded-lg border-border/50 bg-muted/30"
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">End</label>
                <Input
                  type="time"
                  value={endTime}
                  onChange={(event) => updateTimeRange(startTime, event.target.value)}
                  className="time-display w-28 rounded-lg border-border/50 bg-muted/30"
                />
              </div>
            </div>
          </div>

          <Separator orientation="vertical" className="hidden h-16 sm:block" />

          <div className="space-y-3">
            <h4 className="text-sm font-medium text-muted-foreground">Duration</h4>
            <div className="flex gap-3">
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">Hours</label>
                <Input
                  type="number"
                  value={hours}
                  onChange={(event) =>
                    updateTotalTime(parseInt(event.target.value), minutes)
                  }
                  className="w-20 rounded-lg border-border/50 bg-muted/30"
                  min={0}
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">Minutes</label>
                <Input
                  type="number"
                  value={minutes}
                  onChange={(event) =>
                    updateTotalTime(hours, parseInt(event.target.value))
                  }
                  className="w-20 rounded-lg border-border/50 bg-muted/30"
                  min={0}
                  max={59}
                />
              </div>
            </div>
          </div>
        </div>
      </div>

      <div
        className={cn(
          "flex items-center justify-between border-t border-border/50",
          props.variant === "inline"
            ? "bg-muted/20 px-5 py-4"
            : "bg-transparent px-0 pb-0 pt-4",
        )}
      >
        <Button
          variant="ghost"
          size="sm"
          onClick={handleDelete}
          disabled={isDeletingTimeEntry || isUpdatingTimeEntry}
          className="gap-2 text-destructive hover:bg-destructive/10 hover:text-destructive"
        >
          <TrashIcon className="h-4 w-4" />
          Delete
        </Button>
        <div className="flex gap-3">
          <Button variant="outline" size="sm" onClick={props.onCancel} className="rounded-lg">
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={
              isUpdatingTimeEntry ||
              !projectId ||
              !activityId ||
              !startTime ||
              (!endTime && hours === 0 && minutes === 0)
            }
            className="btn-glow gap-2 rounded-lg"
          >
            <SaveIcon className="h-4 w-4" />
            Save
          </Button>
        </div>
      </div>
    </>
  );

  if (props.variant === "dialog") return body;

  return (
    <div className="overflow-hidden rounded-xl border border-primary/30 bg-card shadow-glow-sm">
      <div className="border-b border-border/50 bg-primary/5 px-5 py-4">
        <h3 className="font-display text-lg font-semibold">Edit Entry</h3>
      </div>
      {body}
    </div>
  );
}
