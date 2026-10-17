import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { Loader2, PlayIcon, SwordsIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { apiErrorToast } from "@/lib/api/errors";
import { buddyMutations } from "@/lib/api/mutations/buddy";
import { type BattleResult, buddyApiQueries } from "@/lib/api/queries/buddy";
import { formatBuddyTeam } from "@/lib/buddy/profile";
import { Panel, ResultBadge } from "./panel";
import { QueryError } from "./query-error";
import { TeamIcons } from "./team-icons";

const RESULT_TEXT: Readonly<Record<BattleResult, string>> = {
  win: "You beat today's trainer.",
  loss: "Today's trainer won.",
  draw: "Today's fight was a draw.",
};

/**
 * The trainer everyone can fight once today, and how the user's fight went.
 * Fighting it does not count toward the hourly cooldown.
 */
export function TrainerPanel() {
  const navigate = useNavigate();
  const trainerQuery = useQuery(buddyApiQueries.trainer());
  const fight = buddyMutations.useStartTrainerBattle({
    onError: apiErrorToast("Could not fight the daily trainer"),
  });

  const startFight = () =>
    fight.mutate(undefined, {
      onSuccess: (detail) =>
        void navigate({
          to: "/buddies/battles/$battleId",
          params: { battleId: String(detail.id) },
        }),
    });

  return (
    <Panel
      title="Daily trainer"
      description="One trainer a day, the same for everyone. Fight it once; it does not use up your hourly battle."
    >
      {trainerQuery.isPending ? (
        <div className="flex items-center gap-3" aria-busy="true">
          <Skeleton className="h-[34px] w-16 rounded-md" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-36" />
            <Skeleton className="h-3 w-48" />
          </div>
          <Skeleton className="h-8 w-20" />
        </div>
      ) : trainerQuery.isError ? (
        <QueryError
          message="Could not load today's trainer."
          error={trainerQuery.error}
          onRetry={() => void trainerQuery.refetch()}
        />
      ) : (
        <div className="space-y-3">
          <div className="flex items-center gap-3">
            <TeamIcons team={trainerQuery.data.team} />
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-medium">
                {trainerQuery.data.name}
              </p>
              <p className="truncate text-xs text-muted-foreground">
                {formatBuddyTeam(trainerQuery.data.team)}
              </p>
            </div>
            {trainerQuery.data.result === null && (
              <Button
                type="button"
                size="sm"
                className="h-8 shrink-0 px-3"
                disabled={fight.isPending}
                onClick={startFight}
              >
                {fight.isPending ? (
                  <Loader2
                    aria-hidden
                    className="animate-spin motion-reduce:animate-none"
                  />
                ) : (
                  <SwordsIcon aria-hidden />
                )}
                {fight.isPending ? "Fighting..." : "Fight"}
              </Button>
            )}
          </div>
          {trainerQuery.data.result !== null && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-xl border border-border/60 bg-muted/30 px-3 py-2">
              <ResultBadge result={trainerQuery.data.result} />
              <p className="min-w-0 flex-1 text-sm">
                {RESULT_TEXT[trainerQuery.data.result]}{" "}
                <span className="text-muted-foreground">
                  A new trainer arrives tomorrow.
                </span>
              </p>
              {trainerQuery.data.battleId !== null && (
                <Button
                  asChild
                  variant="outline"
                  size="sm"
                  className="h-8 px-3"
                >
                  <Link
                    to="/buddies/battles/$battleId"
                    params={{ battleId: String(trainerQuery.data.battleId) }}
                  >
                    <PlayIcon aria-hidden />
                    Replay
                  </Link>
                </Button>
              )}
            </div>
          )}
        </div>
      )}
    </Panel>
  );
}
