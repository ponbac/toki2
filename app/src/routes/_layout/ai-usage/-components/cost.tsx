import { CircleHelp } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { AiCost } from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { formatCount } from "../-lib/format";
import { formatMoney, useMoney } from "../-lib/money";

/**
 * The mark of usage without a known price: how many requests it is, and on
 * hover or focus why the figure beside it is not a total. Unpriced usage is
 * never shown as zero.
 */
export function UnpricedChip({
  records,
  className,
}: {
  records: number;
  className?: string;
}) {
  const one = records === 1;
  const requests = `${formatCount(records)} ${one ? "request" : "requests"}`;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          tabIndex={0}
          className={cn(
            "inline-flex shrink-0 items-center gap-1 rounded-full bg-primary/10 px-1.5 py-0.5 align-middle text-[11px] font-semibold leading-none tracking-normal text-primary outline-hidden focus-visible:ring-2 focus-visible:ring-ring",
            className,
          )}
        >
          <CircleHelp aria-hidden className="size-3" />
          <span aria-hidden>{formatCount(records)}</span>
          <span className="sr-only">plus {requests} of unknown cost</span>
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-64 text-xs font-medium">
        {requests} {one ? "has" : "have"} no known price, so{" "}
        {one ? "its" : "their"} cost is unknown, not zero. A figure beside this
        is the known subtotal.
      </TooltipContent>
    </Tooltip>
  );
}

/**
 * An estimated cost in the display currency: the known amount, with a chip
 * for the requests that have no price. Usage with no priced request at all
 * reads "Unknown".
 */
export function Cost({
  cost,
  className,
}: {
  cost: AiCost;
  className?: string;
}) {
  const money = useMoney();
  if (cost.kind === "estimated") {
    return <span className={className}>{formatMoney(money, cost.usd)}</span>;
  }
  return (
    <span className={cn("inline-flex items-center gap-1.5", className)}>
      {cost.knownUsd > 0 ? formatMoney(money, cost.knownUsd) : "Unknown"}
      <UnpricedChip records={cost.unpricedRecords} />
    </span>
  );
}
