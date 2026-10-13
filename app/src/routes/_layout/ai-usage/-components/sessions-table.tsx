import * as React from "react";
import { keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { ArrowDown, Briefcase, CircleDashed, UserRound } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { PERSONAL_LABEL } from "@/lib/ai-usage";
import {
  aiUsageReportQueries,
  type AiProjectAttribution,
  type AiSessionSort,
  type AiUsageReportParams,
  type AiUsageSession,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import { ChartCard } from "./chart-card";
import { Cost } from "./cost";
import { QueryError } from "./query-state";
import { RepoKey } from "./repo-key";
import {
  formatCompact,
  formatCount,
  knownUsd,
  modelLabel,
  sessionHours,
  totalTokens,
} from "../-lib/format";
import { MoneyContext } from "../-lib/money";
import { useDisplayMoney } from "../-lib/use-display-money";

/** How many sessions each page of the listing holds. */
export const SESSIONS_PAGE_SIZE = 25;

/** The order the listing opens in, and the one that groups by day. */
export const DEFAULT_SESSION_SORT: AiSessionSort = "recent";

const COLUMNS = 6;

/**
 * The user's sessions in the range and filters, sortable by recency, cost or
 * tokens. By recency they are grouped under the local day each was last
 * active, in the server's AI usage time zone. The sort stays when filters
 * change, and the previous sessions stay visible, dimmed, until the new ones
 * arrive. Each "Show more" fetches only the next page.
 */
export function SessionsTable({
  params,
  machineLabels,
}: {
  params: AiUsageReportParams;
  machineLabels: ReadonlyMap<string, string>;
}) {
  const [sort, setSort] = React.useState<AiSessionSort>(DEFAULT_SESSION_SORT);
  const query = useInfiniteQuery({
    ...aiUsageReportQueries.aiUsageSessions(params, sort, SESSIONS_PAGE_SIZE),
    placeholderData: keepPreviousData,
  });
  const firstPage = query.data?.pages[0];
  const sessions = query.data?.pages.flatMap((page) => page.sessions) ?? [];
  // The rate of the sessions on screen, also while another range loads.
  const { money } = useDisplayMoney(firstPage?.to);
  const max = Math.max(
    0,
    ...sessions.map((session) => knownUsd(session.totals.cost)),
  );

  return (
    <ChartCard
      title="Sessions"
      description={
        firstPage
          ? `${formatCount(firstPage.total)} ${firstPage.total === 1 ? "session" : "sessions"} in the range. Active hours in ${firstPage.timeZone}.`
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
      ) : money === null ? (
        <p className="text-sm text-muted-foreground" role="status">
          Loading sessions…
        </p>
      ) : (
        <MoneyContext.Provider value={money}>
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
                {sessions.map((session, index) => {
                  const hours = sessionHours(
                    session.firstActiveHour,
                    session.lastActiveHour,
                    firstPage.timeZone,
                  );
                  const previous = sessions[index - 1];
                  const startsDay =
                    sort === "recent" &&
                    (previous === undefined ||
                      sessionHours(
                        previous.firstActiveHour,
                        previous.lastActiveHour,
                        firstPage.timeZone,
                      ).day !== hours.day);
                  return (
                    <React.Fragment
                      key={`${session.machineId}:${session.provider}:${session.sessionKey}`}
                    >
                      {startsDay && (
                        <TableRow className="hover:bg-transparent">
                          <TableHead
                            scope="colgroup"
                            colSpan={COLUMNS}
                            className="h-auto bg-muted/40 py-1.5 text-xs font-semibold text-foreground"
                          >
                            {hours.day}
                          </TableHead>
                        </TableRow>
                      )}
                      <SessionRow
                        session={session}
                        hours={sort === "recent" ? hours.hours : hours.span}
                        machine={
                          machineLabels.get(session.machineId) ??
                          "Unknown machine"
                        }
                        max={max}
                      />
                    </React.Fragment>
                  );
                })}
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
        </MoneyContext.Provider>
      )}
    </ChartCard>
  );
}

/** One session on one line; `max` is the costliest session on screen. */
function SessionRow({
  session,
  hours,
  machine,
  max,
}: {
  session: AiUsageSession;
  hours: string;
  machine: string;
  max: number;
}) {
  const models = session.models.map(modelLabel).join(", ");
  const known = knownUsd(session.totals.cost);
  return (
    <TableRow>
      <TableCell className="whitespace-nowrap py-2 tabular-nums">
        {hours}
      </TableCell>
      <TableCell className="max-w-64 py-2">
        <SessionProjects projects={session.projects} />
      </TableCell>
      <TableCell className="max-w-56 py-2">
        <span className="flex min-w-0 items-center gap-1.5" title={models}>
          <AiProviderIcon provider={session.provider} />
          <span className="sr-only">
            {AI_PROVIDER_LABELS[session.provider]}:{" "}
          </span>
          <span className="truncate text-muted-foreground">{models}</span>
        </span>
      </TableCell>
      <TableCell className="max-w-40 truncate py-2 text-muted-foreground">
        {machine}
      </TableCell>
      <TableCell className="py-2 text-right tabular-nums text-muted-foreground">
        {formatCompact(totalTokens(session.totals.tokens))}
      </TableCell>
      <TableCell className="whitespace-nowrap py-2 text-right tabular-nums">
        <span className="flex items-center justify-end gap-2">
          {known > 0 && max > 0 && (
            <span
              aria-hidden
              className="hidden h-1 min-w-0.5 rounded-full bg-foreground/30 md:block"
              style={{ width: `${(known / max) * 3}rem` }}
            />
          )}
          <Cost cost={session.totals.cost} />
        </span>
      </TableCell>
    </TableRow>
  );
}

const KINDS = {
  project: { icon: Briefcase, label: "Project", className: "" },
  personal: { icon: UserRound, label: PERSONAL_LABEL, className: "" },
  unassigned: {
    icon: CircleDashed,
    label: "Unassigned",
    className: "text-primary",
  },
} as const;

function kindOf(attribution: AiProjectAttribution): keyof typeof KINDS {
  switch (attribution.status) {
    case "mapped":
      return "project";
    case "personal":
      return "personal";
    default:
      return "unassigned";
  }
}

/**
 * What a session worked on, on one line: its first project or repository,
 * marked as a project, personal or unassigned, and a control that lists the
 * others it touched.
 */
function SessionProjects({
  projects,
}: {
  projects: readonly AiProjectAttribution[];
}) {
  const [first, ...rest] = projects;
  if (!first) {
    return <span className="text-muted-foreground">–</span>;
  }
  return (
    <span className="flex min-w-0 items-center gap-2">
      <ProjectLabel attribution={first} />
      {rest.length > 0 && (
        <Popover>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label={`Show all ${formatCount(projects.length)} projects of this session`}
              className="shrink-0 rounded-full border border-border/70 px-1.5 text-[11px] tabular-nums text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            >
              +{formatCount(rest.length)}
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-auto max-w-sm p-3">
            <ul className="space-y-1.5 text-sm">
              {projects.map((project) => (
                <li key={project.projectKey} className="flex min-w-0">
                  <ProjectLabel attribution={project} />
                </li>
              ))}
            </ul>
          </PopoverContent>
        </Popover>
      )}
    </span>
  );
}

/** A project or repository, with the mark of how its usage is attributed. */
function ProjectLabel({ attribution }: { attribution: AiProjectAttribution }) {
  const { icon: Icon, label, className } = KINDS[kindOf(attribution)];
  return (
    <span className="flex min-w-0 items-center gap-2">
      <Icon
        aria-hidden
        className={cn("size-3.5 shrink-0 text-muted-foreground", className)}
      />
      <span className="sr-only">{label}: </span>
      {attribution.status === "mapped" ? (
        <span className="truncate" title={attribution.projectKey}>
          {attribution.project.projectName}
        </span>
      ) : (
        <RepoKey projectKey={attribution.projectKey} />
      )}
    </span>
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
