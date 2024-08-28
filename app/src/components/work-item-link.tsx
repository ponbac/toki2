import { cn, LinkData, workItemUrl } from "@/lib/utils";
import { ConditionalTooltip } from "./ui/tooltip";

type WorkItemLinkProps<T extends LinkData> = {
  data: T;
  text?: string;
  tooltip?: string;
  className?: string;
};

// https://dev.azure.com/ex-change-part/Quote%20Manager/hexagon/_workitems/edit/1489
export function WorkItemLink<T extends LinkData>({
  data,
  text,
  tooltip,
  className,
}: WorkItemLinkProps<T>) {
  return (
    <ConditionalTooltip condition={!!tooltip} content={tooltip}>
      <a
        href={workItemUrl(data)}
        target="_blank"
        rel="noreferrer"
        className={cn("hover:underline", className)}
        onClick={(e) => e.stopPropagation()}
      >
        #{data.id}
        {text && ` - ${text}`}
      </a>
    </ConditionalTooltip>
  );
}
