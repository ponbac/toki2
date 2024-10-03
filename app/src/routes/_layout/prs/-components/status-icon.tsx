import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { ReactNode } from "react";

export function StatusIcon(props: { tooltip: string; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger>{props.children}</TooltipTrigger>
      <TooltipContent>
        <span>{props.tooltip}</span>
      </TooltipContent>
    </Tooltip>
  );
}
