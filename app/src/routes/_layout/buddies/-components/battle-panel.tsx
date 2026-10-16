import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { HourglassIcon, Loader2, SwordsIcon, UsersIcon } from "lucide-react";
import * as React from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { apiErrorToast } from "@/lib/api/errors";
import { buddyMutations } from "@/lib/api/mutations/buddy";
import { type BuddyOpponent, buddyApiQueries } from "@/lib/api/queries/buddy";
import { formatBuddyTeam } from "@/lib/buddy/profile";
import { describeRecord, formatRecord, formatWait } from "../-lib/format";
import { useNow } from "../-lib/use-now";
import { Panel, Pill } from "./panel";
import { QueryError } from "./query-error";
import { TeamIcons } from "./team-icons";

/**
 * Colleagues whose buddies can be battled, the hourly cooldown and a button
 * per colleague. Starting a battle opens its replay.
 */
export function BattlePanel() {
  const navigate = useNavigate();
  const opponentsQuery = useQuery(buddyApiQueries.opponents());
  const startBattle = buddyMutations.useStartBattle({
    onError: apiErrorToast("Could not start the battle"),
  });
  const now = useNow(5_000);

  const nextBattleAt = opponentsQuery.data?.nextBattleAt
    ? Date.parse(opponentsQuery.data.nextBattleAt)
    : null;
  const waitMs = nextBattleAt === null ? 0 : nextBattleAt - now;
  const ready = waitMs <= 0;

  // Once the cooldown is over, ask the server again, which is the judge of it.
  const { refetch } = opponentsQuery;
  React.useEffect(() => {
    if (nextBattleAt === null) {
      return;
    }
    const timeout = window.setTimeout(
      () => void refetch(),
      Math.max(0, nextBattleAt - Date.now()) + 1_000,
    );
    return () => window.clearTimeout(timeout);
  }, [nextBattleAt, refetch]);

  const battle = (opponent: BuddyOpponent) =>
    startBattle.mutate(
      { opponentId: opponent.userId },
      {
        onSuccess: (detail) =>
          void navigate({
            to: "/buddies/battles/$battleId",
            params: { battleId: String(detail.id) },
          }),
      },
    );

  return (
    <Panel
      title="Battle"
      description="Everyone with buddies turned on. You can start one battle an hour, and battle each colleague once a day."
      action={
        opponentsQuery.data &&
        (ready ? (
          <Pill tone="ready">Ready</Pill>
        ) : (
          <Pill tone="waiting">
            <HourglassIcon aria-hidden className="size-3" />
            Next battle in {formatWait(waitMs)}
          </Pill>
        ))
      }
    >
      {opponentsQuery.isPending ? (
        <RowsSkeleton />
      ) : opponentsQuery.isError ? (
        <QueryError
          message="Could not load your colleagues' buddies."
          error={opponentsQuery.error}
          onRetry={() => void opponentsQuery.refetch()}
        />
      ) : opponentsQuery.data.opponents.length === 0 ? (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-border/80 px-4 py-8 text-center">
          <UsersIcon aria-hidden className="size-6 text-muted-foreground" />
          <p className="text-sm font-medium">No colleagues have buddies yet</p>
          <p className="max-w-xs text-xs text-muted-foreground">
            When a colleague turns on their timer buddies, you can battle them
            here.
          </p>
        </div>
      ) : (
        <ul className="divide-y divide-border/60">
          {opponentsQuery.data.opponents.map((opponent) => (
            <OpponentRow
              key={opponent.userId}
              opponent={opponent}
              ready={ready}
              starting={
                startBattle.isPending &&
                startBattle.variables.opponentId === opponent.userId
              }
              busy={startBattle.isPending}
              onBattle={() => battle(opponent)}
            />
          ))}
        </ul>
      )}
    </Panel>
  );
}

function OpponentRow({
  opponent,
  ready,
  starting,
  busy,
  onBattle,
}: {
  opponent: BuddyOpponent;
  ready: boolean;
  /** This colleague's battle is being started. */
  starting: boolean;
  /** Any battle is being started. */
  busy: boolean;
  onBattle: () => void;
}) {
  const { record } = opponent;
  const battles = record.wins + record.losses + record.draws;
  const firstName = opponent.fullName.trim().split(/\s+/)[0] || "them";

  return (
    <li className="flex items-center gap-3 py-2.5 first:pt-0 last:pb-0">
      <TeamIcons team={opponent.team} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{opponent.fullName}</p>
        <p className="truncate text-xs text-muted-foreground">
          {formatBuddyTeam(opponent.team)}
        </p>
      </div>
      {battles > 0 && (
        <Tooltip>
          <TooltipTrigger asChild>
            <span
              tabIndex={0}
              className="shrink-0 rounded font-mono text-xs font-semibold tabular-nums text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {formatRecord(record)}
              <span className="sr-only">
                {" "}
                against {firstName}: {describeRecord(record)}
              </span>
            </span>
          </TooltipTrigger>
          <TooltipContent className="text-xs">
            You against {firstName}: {describeRecord(record)}
          </TooltipContent>
        </Tooltip>
      )}
      {opponent.battledToday ? (
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className="h-8 shrink-0 px-3"
          disabled
        >
          Battled today
        </Button>
      ) : (
        <Button
          type="button"
          size="sm"
          className="h-8 shrink-0 px-3"
          disabled={!ready || busy}
          aria-label={`Battle ${opponent.fullName}`}
          onClick={onBattle}
        >
          {starting ? (
            <Loader2
              aria-hidden
              className="animate-spin motion-reduce:animate-none"
            />
          ) : (
            <SwordsIcon aria-hidden />
          )}
          {starting ? "Battling..." : "Battle"}
        </Button>
      )}
    </li>
  );
}

function RowsSkeleton() {
  return (
    <div className="space-y-3" aria-busy="true" aria-label="Loading">
      {[0, 1, 2].map((row) => (
        <div key={row} className="flex items-center gap-3">
          <Skeleton className="size-[34px] rounded-md" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-32" />
            <Skeleton className="h-3 w-48" />
          </div>
          <Skeleton className="h-8 w-20" />
        </div>
      ))}
    </div>
  );
}
