import type {
  AiCost,
  AiProjectAttribution,
  AiUsageReport,
} from "@/lib/api/queries/ai-usage-report";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AiProviderLabel } from "@/components/ai-provider-icon";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import { cn } from "@/lib/utils";
import { ChartCard } from "./chart-card";
import { knownShare } from "../-lib/aggregate";
import {
  formatCompact,
  formatCount,
  formatPercent,
  modelLabel,
  totalTokens,
} from "../-lib/format";
import { formatCost, useMoney } from "../-lib/money";

/** Usage per project, with the project keys behind each and unassigned usage. */
export function ProjectTable({ report }: { report: AiUsageReport }) {
  const { currency } = useMoney();
  return (
    <ChartCard
      title="By project"
      description={`API-equivalent cost estimates in ${currency}. Unassigned usage has no mapped project. ${PERSONAL_LABEL} usage is yours to keep out of the bill.`}
    >
      {report.projects.length === 0 ? (
        <p className="text-sm text-muted-foreground">No usage in this range.</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Project</TableHead>
              <TableHead className="text-right">Tokens</TableHead>
              <TableHead className="text-right">Est. cost</TableHead>
              <TableHead className="text-right">
                <ShareHeading total={report.totals.cost} />
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {report.projects.map((group) => (
              <TableRow
                key={
                  group.project?.projectId ??
                  (group.personal ? "personal" : "unassigned")
                }
              >
                <TableCell className="max-w-64">
                  <p
                    className={cn(
                      "truncate font-medium",
                      group.project === null && "text-muted-foreground",
                    )}
                  >
                    {group.project?.projectName ??
                      (group.personal ? PERSONAL_LABEL : "Unassigned")}
                  </p>
                  <ul className="mt-0.5 space-y-0.5">
                    {group.keys.map((key) => (
                      <li
                        key={key.projectKey}
                        className="text-xs text-muted-foreground [overflow-wrap:anywhere]"
                      >
                        {key.projectKey}
                        {key.status !== "mapped" && (
                          <span className="whitespace-nowrap">
                            {" "}
                            · {statusLabel(key)}
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                </TableCell>
                <TableCell className="text-right align-top tabular-nums">
                  {formatCompact(totalTokens(group.totals.tokens))}
                </TableCell>
                <CostCell cost={group.totals.cost} />
                <ShareCell
                  cost={group.totals.cost}
                  total={report.totals.cost}
                />
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      <ShareFootnote total={report.totals.cost} />
    </ChartCard>
  );
}

/** Usage per provider and model. */
export function ModelTable({ report }: { report: AiUsageReport }) {
  const { currency } = useMoney();
  return (
    <ChartCard
      title="By model"
      description={`API-equivalent cost estimates in ${currency}, per tool and model.`}
    >
      {report.models.length === 0 ? (
        <p className="text-sm text-muted-foreground">No usage in this range.</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Model</TableHead>
              <TableHead className="text-right">Requests</TableHead>
              <TableHead className="text-right">Tokens</TableHead>
              <TableHead className="text-right">Est. cost</TableHead>
              <TableHead className="text-right">
                <ShareHeading total={report.totals.cost} />
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {report.models.map((model) => (
              <TableRow key={`${model.provider}:${model.model}`}>
                <TableCell className="max-w-64">
                  <p className="truncate font-medium" title={model.model}>
                    {modelLabel(model.model)}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    <AiProviderLabel
                      provider={model.provider}
                      iconClassName="size-3"
                    />
                  </p>
                </TableCell>
                <TableCell className="text-right align-top tabular-nums">
                  {formatCount(model.totals.records)}
                </TableCell>
                <TableCell className="text-right align-top tabular-nums">
                  {formatCompact(totalTokens(model.totals.tokens))}
                </TableCell>
                <CostCell cost={model.totals.cost} />
                <ShareCell
                  cost={model.totals.cost}
                  total={report.totals.cost}
                />
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      <ShareFootnote total={report.totals.cost} />
    </ChartCard>
  );
}

/** Why a project key's usage counts for no project, in a few words. */
function statusLabel(attribution: AiProjectAttribution): string {
  switch (attribution.status) {
    case "mapped":
      return attribution.project.projectName;
    case "stale":
      return "mapping outside this company";
    case "unconfigured":
      return "time tracking not configured";
    case "unmapped":
      return attribution.mappable ? "not mapped yet" : "cannot be mapped";
    case "personal":
      return "left out of the bill";
    case "unattributed":
      return "no project metadata";
  }
}

function CostCell({ cost }: { cost: AiCost }) {
  const money = useMoney();
  return (
    <TableCell className="whitespace-nowrap text-right align-top tabular-nums">
      {formatCost(money, cost)}
      {cost.kind === "partial" && (
        <p className="text-xs text-muted-foreground">
          {formatCount(cost.unpricedRecords)} unpriced
        </p>
      )}
    </TableCell>
  );
}

function ShareCell({ cost, total }: { cost: AiCost; total: AiCost }) {
  // Usage with no known cost at all has no share of the known cost to show.
  const share =
    cost.kind === "partial" && cost.knownUsd === 0
      ? null
      : knownShare(cost, total);
  return (
    <TableCell className="text-right align-top tabular-nums text-muted-foreground">
      {share === null ? "–" : formatPercent(share)}
    </TableCell>
  );
}

/**
 * While any usage is unpriced, shares are of the known cost only: the unknown
 * remainder cannot be shared out, so the heading says so.
 */
function ShareHeading({ total }: { total: AiCost }) {
  return total.kind === "partial" ? (
    <abbr
      className="no-underline"
      title="Share of the known cost; unpriced usage is left out"
    >
      Share*
    </abbr>
  ) : (
    "Share"
  );
}

/** The footnote to a share column while any usage is unpriced. */
function ShareFootnote({ total }: { total: AiCost }) {
  return total.kind === "partial" ? (
    <p className="text-xs text-muted-foreground">
      * Share of the known cost. Unpriced usage has no known cost to share out,
      so it is left out.
    </p>
  ) : null;
}
