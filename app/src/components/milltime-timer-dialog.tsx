import { MessageCircleCodeIcon, CodeXmlIcon } from "lucide-react";
import { title } from "process";
import { AzureAvatar } from "./azure-avatar";
import BranchLink from "./branch-link";
import { PRLink } from "./pr-link";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";

export const MilltimeTimerDialog = () => {
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          navigate({ to: "..", search: parentSearch });
        }
      }}
    >
      <DialogContent className="max-w-5xl">
        <DialogHeader>
          <DialogTitle className="flex flex-row items-center gap-2">
            <AzureAvatar user={createdBy} className="size-8" />
            <PRLink data={props.pullRequest}>
              <h1 className="text-xl font-semibold">{title}</h1>
            </PRLink>
          </DialogTitle>
          <DialogDescription>
            <BranchLink
              sourceBranch={sourceBranch}
              targetBranch={targetBranch}
            />
          </DialogDescription>
        </DialogHeader>
        <Threads pullRequest={pr} />
        <DialogFooter className="pt-2">
          <Button
            autoFocus
            variant="outline"
            size="sm"
            className="flex gap-2"
            onClick={() => copyTimeReportTextToClipboard("review")}
          >
            <MessageCircleCodeIcon className="size-4" />
            Review
          </Button>
          <Button
            autoFocus
            variant="default"
            size="sm"
            className="flex gap-2"
            onClick={() => copyTimeReportTextToClipboard("develop")}
          >
            <CodeXmlIcon className="size-4" />
            Develop
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};
