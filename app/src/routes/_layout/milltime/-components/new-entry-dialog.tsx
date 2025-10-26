import React from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Combobox } from "@/components/combobox";
import { useMilltimeData } from "@/hooks/useMilltimeData";
import dayjs from "dayjs";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Calendar } from "@/components/ui/calendar";
import { getWeekNumber } from "@/lib/utils";
import { CalendarIcon } from "lucide-react";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { CreateProjectRegistrationPayload } from "@/lib/api/mutations/milltime";

export function NewEntryDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (payload: CreateProjectRegistrationPayload) => void;
}) {
  const [projectId, setProjectId] = React.useState("");
  const [activityName, setActivityName] = React.useState("");
  const [note, setNote] = React.useState("");
  const [regDay, setRegDay] = React.useState(dayjs().format("YYYY-MM-DD"));
  const [isDateOpen, setIsDateOpen] = React.useState(false);
  const [startTime, setStartTime] = React.useState("06:00");
  const [endTime, setEndTime] = React.useState("");
  const [hours, setHours] = React.useState(0);
  const [minutes, setMinutes] = React.useState(0);

  const { projects, activities } = useMilltimeData({
    projectId,
    enabled: props.open,
  });
  const selectedProject = projects?.find(
    (p) => p.projectId.toString() === projectId,
  );
  const selectedActivity = activities?.find(
    (a) => a.activityName === activityName,
  );

  const updateTimeRange = (start: string, end: string) => {
    setStartTime(start);
    setEndTime(end);
    if (start && end) {
      const startDt = dayjs(`2000-01-01T${start}`);
      const endDt = dayjs(`2000-01-01T${end}`);
      const diffHours = endDt.diff(startDt, "hour", true);
      setHours(Math.floor(diffHours));
      setMinutes(Math.round((diffHours - Math.floor(diffHours)) * 60));
    }
  };

  const updateTotalTime = (h: number, m: number) => {
    setHours(h);
    setMinutes(m);
    if (startTime) {
      const startDt = dayjs(`2000-01-01T${startTime}`);
      const endDt = startDt.add(h, "hour").add(m, "minute");
      setEndTime(endDt.format("HH:mm"));
    }
  };

  const reset = () => {
    setProjectId("");
    setActivityName("");
    setNote("");
    setRegDay(dayjs().format("YYYY-MM-DD"));
    setStartTime("06:00");
    setEndTime("");
    setHours(0);
    setMinutes(0);
  };

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        props.onOpenChange(open);
        if (!open) reset();
      }}
    >
      <DialogContent className="w-full max-w-2xl">
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (!selectedProject || !selectedActivity) return;
            const startISO = dayjs(`${regDay}T${startTime}`).toISOString();
            const computedEnd = endTime
              ? endTime
              : startTime
                ? dayjs(`2000-01-01T${startTime}`)
                    .add(hours, "hour")
                    .add(minutes, "minute")
                    .format("HH:mm")
                : "";
            const endISO = dayjs(`${regDay}T${computedEnd}`).toISOString();
            props.onCreate({
              projectId: selectedProject.projectId,
              projectName: selectedProject.projectName,
              activityId: selectedActivity.activity,
              activityName: selectedActivity.activityName,
              startTime: startISO,
              endTime: endISO,
              regDay,
              weekNumber: getWeekNumber(new Date(regDay)),
              userNote: note,
            });
          }}
        >
          <DialogHeader>
            <DialogTitle>New Entry</DialogTitle>
            <DialogDescription>
              Create a time entry without starting a timer.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Combobox
              items={
                projects?.map((p) => ({
                  value: p.projectId.toString(),
                  label: p.projectName,
                })) || []
              }
              placeholder="Select project..."
              searchPlaceholder="Search projects..."
              onSelect={(v) => setProjectId(v)}
              emptyMessage="No projects found"
              value={projectId}
              onChange={(v) => setProjectId(v)}
            />
            <Combobox
              items={
                activities?.map((a) => ({
                  value: a.activityName,
                  label: a.activityName,
                })) || []
              }
              placeholder="Select activity..."
              searchPlaceholder="Search activities..."
              onSelect={(v) => setActivityName(v)}
              emptyMessage="No activities found"
              disabled={!projectId}
              value={activityName}
              onChange={(v) => setActivityName(v)}
            />
            <Input
              placeholder="Note"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />

            <div className="flex flex-col gap-6">
              <div className="space-y-4">
                <Popover open={isDateOpen} onOpenChange={setIsDateOpen}>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      className="mt-1 w-full justify-start"
                    >
                      <CalendarIcon className="mr-2 h-4 w-4" />
                      {dayjs(regDay).format("ddd, MMM D, YYYY")}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-auto p-0" align="start">
                    <Calendar
                      mode="single"
                      selected={new Date(regDay)}
                      onSelect={(d) => {
                        if (d) {
                          setRegDay(dayjs(d).format("YYYY-MM-DD"));
                          setIsDateOpen(false);
                        }
                      }}
                      weekStartsOn={1}
                      initialFocus
                    />
                  </PopoverContent>
                </Popover>
              </div>
              <div className="relative mt-2 flex gap-12">
                <div className="space-y-4">
                  <h3 className="font-medium">Range</h3>
                  <div className="flex gap-4">
                    <div className="w-32">
                      <label className="block text-sm font-medium text-muted-foreground">
                        Start Time
                      </label>
                      <Input
                        type="time"
                        value={startTime}
                        onChange={(e) =>
                          updateTimeRange(e.target.value, endTime)
                        }
                        className="mt-1"
                      />
                    </div>
                    <div className="w-32">
                      <label className="block text-sm font-medium text-muted-foreground">
                        End Time
                      </label>
                      <Input
                        type="time"
                        value={endTime}
                        onChange={(e) =>
                          updateTimeRange(startTime, e.target.value)
                        }
                        className="mt-1"
                      />
                    </div>
                  </div>
                </div>

                <Separator
                  orientation="vertical"
                  className="mb-[6px] h-[80px] self-end"
                />

                <div className="space-y-4">
                  <h3 className="font-medium">Total</h3>
                  <div className="flex gap-4">
                    <div className="w-24">
                      <label className="block text-sm font-medium text-muted-foreground">
                        Hours
                      </label>
                      <Input
                        type="number"
                        value={hours}
                        onChange={(e) =>
                          updateTotalTime(parseInt(e.target.value), minutes)
                        }
                        className="mt-1"
                        min={0}
                      />
                    </div>
                    <div className="w-24">
                      <label className="block text-sm font-medium text-muted-foreground">
                        Minutes
                      </label>
                      <Input
                        type="number"
                        value={minutes}
                        onChange={(e) =>
                          updateTotalTime(hours, parseInt(e.target.value))
                        }
                        className="mt-1"
                        min={0}
                        max={59}
                      />
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button
              type="submit"
              size="sm"
              disabled={
                !selectedProject ||
                !selectedActivity ||
                (!endTime && hours === 0 && minutes === 0)
              }
            >
              Create
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
