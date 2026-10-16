import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Settings2Icon } from "lucide-react";
import * as React from "react";
import { z } from "zod";
import { BuddySettingsDialog } from "@/components/timer-buddy/buddy-settings-dialog";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { buddyApiQueries } from "@/lib/api/queries/buddy";
import { BattlePanel } from "./-components/battle-panel";
import { BuddiesOff } from "./-components/buddies-off";
import { LeaderboardPanel } from "./-components/leaderboard-panel";
import { Panel } from "./-components/panel";
import { QueryError } from "./-components/query-error";
import { RecentBattlesPanel } from "./-components/recent-battles-panel";
import { TrainerPanel } from "./-components/trainer-panel";
import { YourBuddies } from "./-components/your-buddies";
import { MONTH_PATTERN } from "./-lib/format";
import { LEADERBOARD_TABS } from "./-lib/leaderboard";

/**
 * The leaderboard's month (`YYYY-MM`, the current one when absent) and tab.
 * A malformed value falls back to its default on its own.
 */
const searchSchema = z.object({
  month: z.string().regex(MONTH_PATTERN).optional().catch(undefined),
  board: z.enum(LEADERBOARD_TABS).optional().catch(undefined),
});

export const Route = createFileRoute("/_layout/buddies/")({
  validateSearch: searchSchema,
  component: BuddiesPage,
});

function BuddiesPage() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const profileQuery = useQuery(buddyApiQueries.profile());
  const [settingsOpen, setSettingsOpen] = React.useState(false);
  const openSettings = () => setSettingsOpen(true);

  const profile = profileQuery.data;
  const enabled = profile?.enabled ?? false;

  return (
    <main className="mx-auto w-full max-w-6xl space-y-5 p-4 md:p-8">
      <header className="space-y-1">
        <h1 className="font-display text-2xl font-semibold tracking-tight">
          Buddies
        </h1>
        <p className="max-w-3xl text-sm text-muted-foreground">
          Your timer buddies level up as you log hours. Battle your colleagues'
          buddies, beat the daily trainer and climb the monthly leaderboard.
        </p>
      </header>

      {profile === undefined ? (
        profileQuery.isError ? (
          <QueryError
            message="Could not load your buddies."
            error={profileQuery.error}
            onRetry={() => void profileQuery.refetch()}
          />
        ) : (
          <PageSkeleton />
        )
      ) : (
        <>
          {/* One panel in every state, so the settings dialog in its header
              stays open while the user turns their buddies on or off. */}
          <Panel
            title="Your buddies"
            action={
              <BuddySettingsDialog
                open={settingsOpen}
                onOpenChange={setSettingsOpen}
                trigger={
                  <Button type="button" variant="outline" size="sm">
                    <Settings2Icon aria-hidden />
                    Settings
                  </Button>
                }
              />
            }
          >
            {profile && enabled ? (
              <YourBuddies profile={profile} onPickPartner={openSettings} />
            ) : (
              <BuddiesOff profile={profile} onOpenSettings={openSettings} />
            )}
          </Panel>

          {enabled && (
            <div className="grid items-start gap-5 lg:grid-cols-2">
              <div className="min-w-0 space-y-5">
                <BattlePanel />
                <TrainerPanel />
              </div>
              <div className="min-w-0 space-y-5">
                <RecentBattlesPanel />
                <LeaderboardPanel
                  month={search.month}
                  tab={search.board ?? "wins"}
                  onMonthChange={(month) =>
                    void navigate({
                      search: (previous) => ({ ...previous, month }),
                      replace: true,
                    })
                  }
                  onTabChange={(board) =>
                    void navigate({
                      search: (previous) => ({ ...previous, board }),
                      replace: true,
                    })
                  }
                />
              </div>
            </div>
          )}
        </>
      )}
    </main>
  );
}

function PageSkeleton() {
  return (
    <div className="space-y-5" aria-busy="true" aria-label="Loading">
      <Skeleton className="h-48 w-full rounded-2xl" />
      <div className="grid gap-5 lg:grid-cols-2">
        <Skeleton className="h-72 rounded-2xl" />
        <Skeleton className="h-72 rounded-2xl" />
      </div>
    </div>
  );
}
