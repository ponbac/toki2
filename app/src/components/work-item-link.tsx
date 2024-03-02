import { cn } from "@/lib/utils";
import { ConditionalTooltip } from "./ui/tooltip";

type LinkData = {
  organization: string;
  project: string;
  repoName: string;
  id: number;
};

type WorkItemLinkProps<T extends LinkData> = {
  data: T;
  tooltip?: string;
  className?: string;
};

// https://dev.azure.com/ex-change-part/Quote%20Manager/hexagon/_workitems/edit/1489
export function WorkItemLink<T extends LinkData>({
  data,
  tooltip,
  className,
}: WorkItemLinkProps<T>) {
  return (
    <ConditionalTooltip condition={!!tooltip} content={tooltip}>
      <a
        href={`https://dev.azure.com/${data.organization}/${data.project}/${data.repoName}/_workitems/edit/${data.id}`}
        target="_blank"
        rel="noreferrer"
        className={cn("hover:underline", className)}
        onClick={(e) => e.stopPropagation()}
      >
        #{data.id}
      </a>
    </ConditionalTooltip>
  );
}
