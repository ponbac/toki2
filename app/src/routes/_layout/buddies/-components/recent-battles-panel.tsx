import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import dayjs from "dayjs";
import relativeTime from "dayjs/plugin/relativeTime";
import { ChevronRightIcon, HistoryIcon } from "lucide-react";
import * as React from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { type BattleSummary, buddyApiQueries } from "@/lib/api/queries/buddy";
import { cn } from "@/lib/utils";
import { Panel, ResultBadge } from "./panel";
import { QueryError } from "./query-error";
import { TeamIcons } from "./team-icons";

dayjs.extend(relativeTime);

/** Battles shown before "Show all". */
const FIRST_BATTLES = 8;

/**
 * The user's battles of the last 30 days, newest first. Battles a colleague
 * started against the user are marked until their replay is opened.
 */
export function RecentBattlesPanel() {
  const battlesQuery = useQuery(buddyApiQueries.battles());
  const [showAll, setShowAll] = React.useState(false);

  const battles = battlesQuery.data ?? [];
  const unseen = battles.filter(isUnseenDefense).length;
  const shown = showAll ? battles : battles.slice(0, FIRST_BATTLES);

  return (
    <Panel
      title="Recent battles"
      description="The last 30 days. Open one to watch the replay."
      action={
        unseen > 0 && (
          <span className="rounded-full bg-primary px-2 py-0.5 text-xs font-semibold text-primary-foreground">
            {unseen} new
          </span>
        )
      }
    >
      {battlesQuery.isPending ? (
        <div className="space-y-3" aria-busy="true" aria-label="Loading">
          {[0, 1, 2].map((row) => (
            <div key={row} className="flex items-center gap-3">
              <Skeleton className="h-5 w-11" />
              <div className="flex-1 space-y-1.5">
                <Skeleton className="h-3.5 w-40" />
                <Skeleton className="h-3 w-24" />
              </div>
              <Skeleton className="h-7 w-28" />
            </div>
          ))}
        </div>
      ) : battlesQuery.isError ? (
        <QueryError
          message="Could not load your battles."
          error={battlesQuery.error}
          onRetry={() => void battlesQuery.refetch()}
        />
      ) : battles.length === 0 ? (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-border/80 px-4 py-8 text-center">
          <HistoryIcon aria-hidden className="size-6 text-muted-foreground" />
          <p className="text-sm font-medium">No battles yet</p>
          <p className="max-w-xs text-xs text-muted-foreground">
            Battles you start, and battles colleagues start against your
            buddies, show up here.
          </p>
        </div>
      ) : (
        <>
          <ul className="-mx-2 space-y-0.5">
            {shown.map((battle) => (
              <BattleRow key={battle.id} battle={battle} />
            ))}
          </ul>
          {battles.length > FIRST_BATTLES && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="mt-2 w-full text-muted-foreground"
              aria-expanded={showAll}
              onClick={() => setShowAll((previous) => !previous)}
            >
              {showAll ? "Show fewer" : `Show all ${battles.length}`}
            </Button>
          )}
        </>
      )}
    </Panel>
  );
}

function isUnseenDefense(battle: BattleSummary): boolean {
  return battle.role === "defender" && !battle.seen;
}

/** What happened, from the user's side. */
function battleTitle(battle: BattleSummary): string {
  if (battle.kind === "trainer") {
    return battle.opponent.name;
  }
  return battle.role === "challenger"
    ? `You battled ${battle.opponent.name}`
    : `${battle.opponent.name} battled you`;
}

function BattleRow({ battle }: { battle: BattleSummary }) {
  const created = dayjs(battle.createdAt);
  const unseen = isUnseenDefense(battle);

  return (
    <li>
      <Link
        to="/buddies/battles/$battleId"
        params={{ battleId: String(battle.id) }}
        className="group flex items-center gap-3 rounded-lg px-2 py-2 outline-none transition-colors hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring motion-reduce:transition-none"
      >
        <ResultBadge result={battle.result} />
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-2 text-sm font-medium">
            <span className="truncate">{battleTitle(battle)}</span>
            {unseen && (
              <span className="shrink-0 rounded-full bg-primary px-1.5 py-px text-[10px] font-bold uppercase tracking-wide text-primary-foreground">
                New
              </span>
            )}
          </p>
          <p className="truncate text-xs text-muted-foreground">
            {battle.kind === "trainer" && "Daily trainer · "}
            <time
              dateTime={battle.createdAt}
              title={created.format("YYYY-MM-DD HH:mm")}
            >
              {created.fromNow()}
            </time>
          </p>
          {/* Narrow screens keep the teams under the title, which needs the room. */}
          <Teams battle={battle} className="mt-1.5 flex sm:hidden" />
        </div>
        <Teams battle={battle} className="hidden sm:flex" />
        <ChevronRightIcon
          aria-hidden
          className="hidden size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5 motion-reduce:transition-none sm:block"
        />
      </Link>
    </li>
  );
}

/** Both teams of a battle, the user's first. */
function Teams({
  battle,
  className,
}: {
  battle: BattleSummary;
  className?: string;
}) {
  return (
    <span className={cn("shrink-0 items-center gap-1.5", className)}>
      <TeamIcons team={battle.myTeam} size={28} />
      <span className="text-[10px] font-semibold uppercase text-muted-foreground">
        vs
      </span>
      <TeamIcons team={battle.theirTeam} size={28} />
    </span>
  );
}
