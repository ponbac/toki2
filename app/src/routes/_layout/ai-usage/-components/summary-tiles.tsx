import type { AiUsageReport } from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { knownShare } from "../-lib/aggregate";
import {
  formatCompact,
  formatCost,
  formatCount,
  formatPercent,
  totalTokens,
} from "../-lib/format";

/** The headline figures of the range: cost, tokens, requests and unassigned usage. */
export function SummaryTiles({ report }: { report: AiUsageReport }) {
  const { totals } = report;
  const unassigned = report.projects.find((group) => group.project === null);
  const unassignedShare = unassigned
    ? knownShare(unassigned.totals.cost, totals.cost)
    : null;
  const tokens = totalTokens(totals.tokens);

  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <Tile
        label="Estimated cost"
        value={formatCost(totals.cost)}
        detail={
          totals.cost.kind === "partial"
            ? `${formatCount(totals.cost.unpricedRecords)} requests have no known price, so the total is unknown; the figure is the known subtotal.`
            : "API-equivalent, in USD. Not a subscription bill."
        }
        warning={totals.cost.kind === "partial"}
      />
      <Tile
        label="Tokens"
        value={formatCompact(tokens)}
        detail={`${formatCompact(totals.tokens.cacheRead)} of them cache reads`}
      />
      <Tile
        label="Requests"
        value={formatCount(totals.records)}
        detail={`${new Set(report.days.map((day) => day.date)).size} active days`}
      />
      <Tile
        label="Unassigned"
        value={unassigned ? formatCost(unassigned.totals.cost) : "None"}
        detail={
          unassigned
            ? unassignedShare === null
              ? "Usage without a mapped project"
              : `${formatPercent(unassignedShare)} of the known cost has no mapped project`
            : "All usage counts for a project"
        }
      />
    </div>
  );
}

function Tile({
  label,
  value,
  detail,
  warning = false,
}: {
  label: string;
  value: string;
  detail: string;
  warning?: boolean;
}) {
  return (
    <div className="rounded-2xl border border-border/70 bg-card p-4 shadow-sm">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-2xl font-semibold tracking-tight">{value}</p>
      <p
        className={cn(
          "mt-1 text-xs",
          warning ? "text-foreground" : "text-muted-foreground",
        )}
      >
        {detail}
      </p>
    </div>
  );
}
