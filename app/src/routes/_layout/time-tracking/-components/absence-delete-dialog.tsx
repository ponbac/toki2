import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { apiErrorToast } from "@/lib/api/errors";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import type { AbsenceEntry } from "@/lib/api/queries/time-tracking";
import { toast } from "sonner";

export function AbsenceDeleteDialog({
  absence,
  onOpenChange,
}: {
  absence: AbsenceEntry | null;
  onOpenChange: (open: boolean) => void;
}) {
  const { mutate: deleteAbsence, isPending: isDeleting } =
    timeTrackingMutations.useDeleteAbsence({
      onSuccess: () => {
        onOpenChange(false);
        toast.success("Absence deleted");
      },
      onError: apiErrorToast("Failed to delete absence"),
    });

  return (
    <Dialog open={Boolean(absence)} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Delete absence?</DialogTitle>
          <DialogDescription>
            This removes the absence entry from Kleer.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={!absence || isDeleting}
            onClick={() => {
              if (!absence) return;
              deleteAbsence({
                absenceId: absence.absenceId,
                date: absence.date,
              });
            }}
          >
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
