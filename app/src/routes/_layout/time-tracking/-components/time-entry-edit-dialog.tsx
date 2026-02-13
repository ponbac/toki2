import { TimeEntry } from "@/lib/api/queries/time-tracking";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TimeEntryEditContent } from "./time-entry-edit-content";

type TimeEntryEditDialogProps = {
  entry: TimeEntry | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
};

export function TimeEntryEditDialog(props: TimeEntryEditDialogProps) {
  if (!props.entry) return null;

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle className="font-display text-lg font-semibold">
            Edit Entry
          </DialogTitle>
          <DialogDescription>
            Update project, activity, date, note, and time range for this entry.
          </DialogDescription>
        </DialogHeader>
        <TimeEntryEditContent
          entry={props.entry}
          variant="dialog"
          onSaved={props.onSaved}
          onCancel={() => props.onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
