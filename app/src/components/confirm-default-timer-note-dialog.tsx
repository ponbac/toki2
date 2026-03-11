import { Button } from "./ui/button";
import { SaveIcon } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

export function ConfirmDefaultTimerNoteDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  isPending?: boolean;
}) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-md border-amber-500/20 bg-gradient-to-br from-background via-background to-amber-500/5 shadow-[0_20px_80px_rgba(245,158,11,0.12)]">
        <DialogHeader>
          <DialogTitle>Save timer with default note?</DialogTitle>
          <DialogDescription>
            This note matches one of the default texts used when starting a new
            timer. Save anyway?
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="default"
            onClick={() => props.onOpenChange(false)}
            disabled={props.isPending}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={props.onConfirm}
            disabled={props.isPending}
            className="bg-amber-500/8 hover:bg-amber-500/14 border-amber-500/30 text-amber-700 hover:text-amber-800 dark:text-amber-300 dark:hover:text-amber-200"
          >
            <SaveIcon className="size-4" />
            Save anyway
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
