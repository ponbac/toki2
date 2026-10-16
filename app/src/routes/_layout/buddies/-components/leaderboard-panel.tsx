import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { ChevronLeftIcon, ChevronRightIcon, TrophyIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  type BuddyLeaderboardRow,
  buddyApiQueries,
} from "@/lib/api/queries/buddy";
import { userQueries } from "@/lib/api/queries/user";
import { cn } from "@/lib/utils";
import { monthLabel, shiftMonth } from "../-lib/format";
import {
  LEADERBOARD_TAB_LABELS,
  LEADERBOARD_TABS,
  type LeaderboardTab,
  MIN_RATED_BATTLES,
  battleCount,
  rankLeaderboard,
  unratedRows,
} from "../-lib/leaderboard";
import { Panel } from "./panel";
import { QueryError } from "./query-error";
import { TeamIcons } from "./team-icons";

/** Names listed in the win rate footnote before the rest are counted. */
const MAX_UNRATED_NAMES = 4;

/**
 * The monthly leaderboard of started battles. `month` is `YYYY-MM`, or
 * `undefined` for the server's current month, which is as far forward as it
 * goes.
 */
export function LeaderboardPanel({
  month,
  tab,
  onMonthChange,
  onTabChange,
}: {
  month: string | undefined;
  tab: LeaderboardTab;
  onMonthChange: (month: string | undefined) => void;
  onTabChange: (tab: LeaderboardTab) => void;
}) {
  const currentQuery = useQuery(buddyApiQueries.leaderboard());
  const boardQuery = useQuery({
    ...buddyApiQueries.leaderboard(month),
    placeholderData: keepPreviousData,
  });
  const { data: meId } = useQuery({
    ...userQueries.me(),
    select: (me) => me.id,
  });

  const currentMonth = currentQuery.data?.month;
  const shownMonth = month ?? currentMonth;
  const label =
    shownMonth && currentMonth ? monthLabel(shownMonth, currentMonth) : null;
  const canGoForward =
    month !== undefined && currentMonth !== undefined && month < currentMonth;

  const goForward = () => {
    if (month === undefined || currentMonth === undefined) {
      return;
    }
    const next = shiftMonth(month, 1);
    onMonthChange(next >= currentMonth ? undefined : next);
  };

  return (
    <Panel
      title={label ? `Leaderboard · ${label}` : "Leaderboard"}
      description="Battles you start count. It starts over every month."
      action={
        <div className="flex items-center gap-1">
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="size-8"
            aria-label="Previous month"
            disabled={shownMonth === undefined}
            onClick={() =>
              shownMonth !== undefined &&
              onMonthChange(shiftMonth(shownMonth, -1))
            }
          >
            <ChevronLeftIcon />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon"
            className="size-8"
            aria-label="Next month"
            disabled={!canGoForward}
            onClick={goForward}
          >
            <ChevronRightIcon />
          </Button>
        </div>
      }
    >
      <Tabs
        value={tab}
        onValueChange={(value) => {
          const next = LEADERBOARD_TABS.find(
            (candidate) => candidate === value,
          );
          if (next) {
            onTabChange(next);
          }
        }}
      >
        <TabsList className="h-9">
          {LEADERBOARD_TABS.map((value) => (
            <TabsTrigger key={value} value={value} className="px-2.5 text-xs">
              {LEADERBOARD_TAB_LABELS[value]}
            </TabsTrigger>
          ))}
        </TabsList>
        {boardQuery.isPending ? (
          <BoardSkeleton />
        ) : boardQuery.isError && !boardQuery.data ? (
          <QueryError
            message="Could not load the leaderboard."
            error={boardQuery.error}
            onRetry={() => void boardQuery.refetch()}
          />
        ) : (
          LEADERBOARD_TABS.map((value) => (
            <TabsContent
              key={value}
              value={value}
              className={cn(
                "mt-3 transition-opacity motion-reduce:transition-none",
                boardQuery.isPlaceholderData && "opacity-60",
              )}
              aria-busy={boardQuery.isPlaceholderData}
            >
              <Board
                rows={boardQuery.data.rows}
                tab={value}
                monthName={label ?? "this month"}
                past={month !== undefined && month !== currentMonth}
                meId={meId}
              />
            </TabsContent>
          ))
        )}
      </Tabs>
    </Panel>
  );
}

function Board({
  rows,
  tab,
  monthName,
  past,
  meId,
}: {
  rows: readonly BuddyLeaderboardRow[];
  tab: LeaderboardTab;
  monthName: string;
  /** The month is over, so nothing more will happen in it. */
  past: boolean;
  meId: number | undefined;
}) {
  const entries = rankLeaderboard(rows, tab);

  return (
    <>
      {entries.length === 0 ? (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-border/80 px-4 py-6 text-center">
          <TrophyIcon aria-hidden className="size-5 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            {emptyMessage(rows, tab, monthName, past)}
          </p>
        </div>
      ) : (
        <ol className="divide-y divide-border/60">
          {entries.map((entry) => {
            const isMe = entry.row.userId === meId;
            return (
              <li
                key={entry.row.userId}
                aria-current={isMe ? "true" : undefined}
                className={cn(
                  "flex items-center gap-3 py-2",
                  isMe &&
                    "-mx-2 rounded-lg border-transparent bg-primary/10 px-2 ring-1 ring-inset ring-primary/20",
                )}
              >
                <span className="w-5 shrink-0 text-right font-mono text-xs font-semibold tabular-nums text-muted-foreground">
                  {entry.rank}
                </span>
                <TeamIcons team={entry.row.team} size={28} />
                <span className="min-w-0 flex-1 truncate text-sm">
                  {entry.row.fullName}
                  {isMe && (
                    <span className="text-muted-foreground"> (you)</span>
                  )}
                </span>
                <span
                  className="shrink-0 font-mono text-sm font-semibold tabular-nums"
                  title={entry.description}
                >
                  {entry.score}
                  <span className="sr-only">: {entry.description}</span>
                </span>
              </li>
            );
          })}
        </ol>
      )}
      <p className="mt-3 border-t border-dashed border-border/80 pt-3 text-xs text-muted-foreground">
        {footnote(rows, tab)}
      </p>
    </>
  );
}

function emptyMessage(
  rows: readonly BuddyLeaderboardRow[],
  tab: LeaderboardTab,
  monthName: string,
  past: boolean,
): string {
  const yet = past ? "" : " yet";
  switch (tab) {
    case "wins":
      return `No battles in ${monthName}${yet}.`;
    case "rate":
      return rows.some((row) => battleCount(row) > 0)
        ? `Nobody ${past ? "had" : "has"} ${MIN_RATED_BATTLES} battles in ${monthName}${yet}.`
        : `No battles in ${monthName}${yet}.`;
    case "trainer":
      return `Nobody beat the daily trainer in ${monthName}${yet}.`;
  }
}

function footnote(
  rows: readonly BuddyLeaderboardRow[],
  tab: LeaderboardTab,
): string {
  switch (tab) {
    case "wins":
      return "Ranked by wins, then by the fewest losses. Scores are wins–losses, with draws third when there are any.";
    case "rate": {
      const rule = `Only colleagues with at least ${MIN_RATED_BATTLES} battles in the month are ranked.`;
      const unrated = unratedRows(rows);
      if (unrated.length === 0) {
        return rule;
      }
      const named = unrated
        .slice(0, MAX_UNRATED_NAMES)
        .map((row) => `${row.fullName} (${battleCount(row)})`);
      const rest = unrated.length - named.length;
      const list = new Intl.ListFormat("en", {
        style: "long",
        type: "conjunction",
      }).format(rest > 0 ? [...named, `${rest} more`] : named);
      return `${rule} Not yet: ${list}.`;
    }
    case "trainer":
      return "Days in the month you beat the daily trainer. Everyone faces the same team each day, so it is the fairest board.";
  }
}

function BoardSkeleton() {
  return (
    <div className="mt-3 space-y-3" aria-busy="true" aria-label="Loading">
      {[0, 1, 2, 3].map((row) => (
        <div key={row} className="flex items-center gap-3">
          <Skeleton className="h-3 w-5" />
          <Skeleton className="size-7 rounded-md" />
          <Skeleton className="h-3.5 flex-1" />
          <Skeleton className="h-3.5 w-10" />
        </div>
      ))}
    </div>
  );
}
