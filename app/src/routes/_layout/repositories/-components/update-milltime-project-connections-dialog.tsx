import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { mutations } from "@/lib/api/mutations/mutations";
import { Differ } from "@/lib/api/queries/differs";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { Plus } from "lucide-react";

export function UpdateMilltimeProjectConnectionsDialog(props: {
  open: boolean;
  onClose: () => void;
  differ: Differ;
}) {
  const { data: projects } = milltimeQueries.listProjects({
    showAll: true,
  });

  const { mutate: updateMilltimeProjects, isPending: isUpdating } =
    mutations.useUpdateMilltimeProjects();

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open) {
          props.onClose();
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add new repository</DialogTitle>
          <DialogDescription className="text-balance">
            You can find the required information by inspecting your DevOps URL:{" "}
            <code>
              dev.azure.com/[organization]/[project]/_git/[repository]
            </code>
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2"></div>
        <DialogFooter>
          <Button
            type="submit"
            size="sm"
            className="flex items-center gap-1.5 transition-colors"
            disabled={isUpdating}
          >
            <Plus size="1.25rem" />
            Add repository
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
