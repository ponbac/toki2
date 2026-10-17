import { useQuery } from "@tanstack/react-query";
import { Link, createFileRoute } from "@tanstack/react-router";
import dayjs from "dayjs";
import { HTTPError } from "ky";
import { ArrowLeftIcon, SearchXIcon } from "lucide-react";
import * as React from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { buddyMutations } from "@/lib/api/mutations/buddy";
import { type BattleDetail, buddyApiQueries } from "@/lib/api/queries/buddy";
import { formatBuddyTeam } from "@/lib/buddy/profile";
import { QueryError } from "../-components/query-error";
import { TeamIcons } from "../-components/team-icons";

/** The player, with the pixel font and its effects, loads with the first replay. */
const BattlePlayer = React.lazy(
  () => import("../-components/battle-player/battle-player"),
);

export const Route = createFileRoute("/_layout/buddies/battles/$battleId")({
  component: BattleReplayPage,
});

/** The largest battle id, the server's 32-bit serial. */
const MAX_BATTLE_ID = 2_147_483_647;

/** A battle id from the URL, or `null` when it cannot be one. */
function parseBattleId(value: string): number | null {
  if (!/^[1-9]\d{0,9}$/.test(value)) {
    return null;
  }
  const id = Number(value);
  return id <= MAX_BATTLE_ID ? id : null;
}

/** The server answers 404 for a battle the user did not take part in, too. */
function isMissing(error: unknown): boolean {
  return (
    error instanceof HTTPError &&
    (error.response.status === 404 || error.response.status === 403)
  );
}

function BattleReplayPage() {
  const { battleId } = Route.useParams();
  const id = parseBattleId(battleId);

  return (
    <main className="mx-auto w-full max-w-4xl space-y-5 p-4 md:p-8">
      <Button asChild variant="ghost" size="sm" className="-ml-2">
        <Link to="/buddies">
          <ArrowLeftIcon aria-hidden />
          Buddies
        </Link>
      </Button>
      {id === null ? <MissingBattle /> : <BattleReplay battleId={id} />}
    </main>
  );
}

function BattleReplay({ battleId }: { battleId: number }) {
  const battleQuery = useQuery({
    ...buddyApiQueries.battle(battleId),
    retry: (failures, error) => !isMissing(error) && failures < 2,
  });
  const battle = battleQuery.data;
  useMarkSeen(battle);

  if (!battle) {
    if (battleQuery.isError) {
      return isMissing(battleQuery.error) ? (
        <MissingBattle />
      ) : (
        <QueryError
          message="Could not load this battle."
          error={battleQuery.error}
          onRetry={() => void battleQuery.refetch()}
        />
      );
    }
    return <ReplaySkeleton />;
  }

  return (
    <>
      <ReplayHeader battle={battle} />
      <React.Suspense fallback={<PlayerSkeleton />}>
        <BattlePlayer battle={battle} />
      </React.Suspense>
    </>
  );
}

/**
 * Tells the server the user has seen a battle a colleague started against
 * them, once, when its replay opens. A failure only leaves the battle marked
 * as new, so it is logged rather than shown.
 */
function useMarkSeen(battle: BattleDetail | undefined) {
  const { mutate: markSeen } = buddyMutations.useMarkBattleSeen({
    onError: (error) => {
      const status = error instanceof HTTPError ? error.response.status : null;
      console.warn(
        `Could not mark the battle as seen (${status ?? "no response"})`,
      );
    },
  });
  const requested = React.useRef(new Set<number>());

  const unseenId =
    battle && battle.role === "defender" && !battle.seen ? battle.id : null;
  React.useEffect(() => {
    if (unseenId === null || requested.current.has(unseenId)) {
      return;
    }
    requested.current.add(unseenId);
    markSeen({ battleId: unseenId });
  }, [unseenId, markSeen]);
}

/** Who battled whom and when; how it ended is left to the replay. */
function ReplayHeader({ battle }: { battle: BattleDetail }) {
  const opponent =
    battle.role === "challenger" ? battle.defender : battle.challenger;
  const title =
    battle.kind === "trainer"
      ? `Daily trainer: ${battle.defender.name}`
      : battle.role === "challenger"
        ? `You battled ${opponent.name}`
        : `${opponent.name} battled your buddies`;
  const created = dayjs(battle.createdAt);

  return (
    <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
      <div className="min-w-0 space-y-1">
        <h1 className="font-display text-2xl font-semibold tracking-tight">
          {title}
        </h1>
        <p className="text-sm text-muted-foreground">
          <time dateTime={battle.createdAt}>
            {created.format("dddd D MMMM YYYY, HH:mm")}
          </time>
        </p>
      </div>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <TeamIcons team={battle.myTeam} size={28} />
        <span className="font-semibold uppercase">vs</span>
        <TeamIcons team={battle.theirTeam} size={28} />
        <span className="sr-only">
          Your team: {formatBuddyTeam(battle.myTeam)}. Their team:{" "}
          {formatBuddyTeam(battle.theirTeam)}.
        </span>
      </div>
    </header>
  );
}

function MissingBattle() {
  return (
    <div className="flex flex-col items-center gap-3 rounded-2xl border border-dashed border-border/80 px-6 py-12 text-center">
      <SearchXIcon aria-hidden className="size-8 text-muted-foreground" />
      <h1 className="font-display text-xl font-semibold">Battle not found</h1>
      <p className="max-w-sm text-sm text-muted-foreground">
        This battle does not exist, or you did not take part in it.
      </p>
      <Button asChild variant="outline" size="sm">
        <Link to="/buddies">Back to Buddies</Link>
      </Button>
    </div>
  );
}

function ReplaySkeleton() {
  return (
    <div className="space-y-5" aria-busy="true" aria-label="Loading">
      <div className="space-y-2">
        <Skeleton className="h-7 w-72" />
        <Skeleton className="h-4 w-48" />
      </div>
      <PlayerSkeleton />
    </div>
  );
}

function PlayerSkeleton() {
  return <Skeleton className="aspect-[3/2] w-full rounded-2xl" />;
}
