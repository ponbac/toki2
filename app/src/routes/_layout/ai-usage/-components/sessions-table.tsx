import * as React from "react";
import { keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { ArrowDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import {
  aiUsageReportQueries,
  type AiSessionSort,
  type AiUsageReportParams,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { ChartCard } from "./chart-card";
import { QueryError } from "./query-state";
import { PROVIDER_COLORS } from "../-lib/colors";
import {
  attributionLabel,
  formatCompact,
  formatCost,
  formatCount,
  formatHourSpan,
  modelLabel,
  totalTokens,
} from "../-lib/format";

const PAGE_SIZE = 25;

/**
 * The user's sessions in the range and filters, sortable by recency, cost or
 * tokens. Hours are shown in the server's AI usage time zone. The sort stays
 * when filters change, and the previous sessions stay visible, dimmed, until
 * the new ones arrive. Each "Show more" fetches only the next page.
 */
export function SessionsTable({
  params,
  machineLabels,
}: {
  params: AiUsageReportParams;
  machineLabels: ReadonlyMap<string, string>;
}) {
  const [sort, setSort] = React.useState<AiSessionSort>("recent");
  const query = useInfiniteQuery({
    ...aiUsageReportQueries.aiUsageSessions(params, sort, PAGE_SIZE),
    placeholderData: keepPreviousData,
  });
  const firstPage = query.data?.pages[0];
  const sessions = query.data?.pages.flatMap((page) => page.sessions) ?? [];

  return (
    <ChartCard
      title="Sessions"
      description={
        firstPage
          ? `${formatCount(firstPage.total)} ${firstPage.total === 1 ? "session" : "sessions"} in the range. Active hours in ${firstPage.timeZone}; costs are API-equivalent estimates in USD.`
          : "Your sessions in the range."
      }
    >
      {query.isPending ? (
        <p className="text-sm text-muted-foreground" role="status">
          Loading sessions…
        </p>
      ) : query.isError ? (
        <QueryError
          message="Could not load your sessions."
          error={query.error}
          onRetry={() => void query.refetch()}
        />
      ) : firstPage === undefined || sessions.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No sessions in this range.
        </p>
      ) : (
        <div
          className={cn(
            "space-y-3 transition-opacity",
            query.isPlaceholderData && "opacity-60",
          )}
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>
                  <SortButton
                    active={sort === "recent"}
                    onClick={() => setSort("recent")}
                  >
                    Active hours
                  </SortButton>
                </TableHead>
                <TableHead>Project</TableHead>
                <TableHead>Tool and models</TableHead>
                <TableHead>Machine</TableHead>
                <TableHead className="text-right">
                  <SortButton
                    active={sort === "tokens"}
                    onClick={() => setSort("tokens")}
                  >
                    Tokens
                  </SortButton>
                </TableHead>
                <TableHead className="text-right">
                  <SortButton
                    active={sort === "cost"}
                    onClick={() => setSort("cost")}
                  >
                    Est. cost
                  </SortButton>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sessions.map((session) => (
                <TableRow
                  key={`${session.machineId}:${session.provider}:${session.sessionKey}`}
                >
                  <TableCell className="whitespace-nowrap align-top text-sm tabular-nums">
                    {formatHourSpan(
                      session.firstActiveHour,
                      session.lastActiveHour,
                      firstPage.timeZone,
                    )}
                  </TableCell>
                  <TableCell className="max-w-56 align-top">
                    {session.projects.map((project) => (
                      <div key={project.projectKey} className="min-w-0">
                        <p
                          className="truncate text-sm"
                          title={project.projectKey}
                        >
                          {attributionLabel(project)}
                        </p>
                        {project.status !== "mapped" && (
                          <p className="text-xs text-muted-foreground">
                            Unassigned
                          </p>
                        )}
                      </div>
                    ))}
                  </TableCell>
                  <TableCell className="max-w-56 align-top">
                    <p className="flex items-center gap-1.5 text-sm">
                      <span
                        aria-hidden
                        className="size-2 shrink-0 rounded-sm"
                        style={{
                          backgroundColor: PROVIDER_COLORS[session.provider],
                        }}
                      />
                      {AI_PROVIDER_LABELS[session.provider]}
                    </p>
                    <p
                      className="truncate text-xs text-muted-foreground"
                      title={session.models.map(modelLabel).join(", ")}
                    >
                      {session.models.map(modelLabel).join(", ")}
                    </p>
                  </TableCell>
                  <TableCell className="max-w-40 truncate align-top text-sm text-muted-foreground">
                    {machineLabels.get(session.machineId) ?? "Unknown machine"}
                  </TableCell>
                  <TableCell className="text-right align-top tabular-nums">
                    {formatCompact(totalTokens(session.totals.tokens))}
                  </TableCell>
                  <TableCell className="whitespace-nowrap text-right align-top tabular-nums">
                    {formatCost(session.totals.cost)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
            <span>
              Showing {formatCount(sessions.length)} of{" "}
              {formatCount(firstPage.total)}
              {sort === "cost" &&
                ". Sessions with unpriced usage rank by their known subtotal."}
            </span>
            {query.hasNextPage && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={query.isFetchingNextPage || query.isPlaceholderData}
                onClick={() => void query.fetchNextPage()}
              >
                {query.isFetchingNextPage ? "Loading…" : "Show more"}
              </Button>
            )}
          </div>
        </div>
      )}
    </ChartCard>
  );
}

function SortButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "inline-flex items-center gap-1 hover:text-foreground",
        active && "text-foreground",
      )}
    >
      {children}
      <ArrowDown
        aria-hidden
        className={cn("size-3", active ? "opacity-100" : "opacity-0")}
      />
    </button>
  );
}
