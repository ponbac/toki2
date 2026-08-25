import { cn } from "@/lib/utils";
import { ConditionalTooltip } from "./ui/tooltip";

export function WorkItemLink({
  data,
  text,
  tooltip,
  className,
}: {
  data: { id: string | number; url: string };
  text?: string;
  tooltip?: string;
  className?: string;
}) {
  return (
    <ConditionalTooltip condition={!!tooltip} content={tooltip}>
      <a
        href={data.url}
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
