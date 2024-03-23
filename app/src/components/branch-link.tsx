import { toast } from "sonner";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { ClipboardCopy } from "lucide-react";

export default function BranchLink(props: {
  sourceBranch: string;
  targetBranch: string;
}) {
  const trimmedSourceBranch = trimBranch(props.sourceBranch);
  const trimmedTargetBranch = trimBranch(props.targetBranch);

  return (
    <Tooltip>
      <TooltipTrigger
        className="hover:underline"
        onClick={() => {
          navigator.clipboard.writeText(trimmedSourceBranch);
          toast.info(
            <div className="flex flex-row items-center">
              <ClipboardCopy className="mr-2 inline-block" size="1.25rem" />
              <p className="text-pretty">
                Copied <span className="font-mono">{trimmedSourceBranch}</span>{" "}
                to clipboard
              </p>
            </div>,
          );
        }}
      >
        <span className="font-mono text-sm">
          {trimmedSourceBranch} → {trimmedTargetBranch}
        </span>
      </TooltipTrigger>
      <TooltipContent>
        <div className="flex flex-row items-center text-sm">
          <ClipboardCopy className="mr-2 inline-block" size="1rem" />
          Copy source branch
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

function trimBranch(branch: string) {
  return branch.replace("refs/heads/", "");
}
