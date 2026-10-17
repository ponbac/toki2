import { EyeOffIcon, LockIcon, PlusIcon } from "lucide-react";
import { BuddyPreview } from "@/components/timer-buddy/buddy-preview";
import { TodayXp } from "@/components/timer-buddy/today-xp";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { Buddy, BuddyProfile } from "@/lib/api/queries/buddy";
import { formatBuddyXp } from "@/lib/buddy/format";
import { buddyLevelProgress, buddyXpForLevel } from "@/lib/buddy/level";
import { buddySpeciesName } from "@/lib/buddy/species";

/**
 * The user's lead and partner with their levels, and the XP they got today.
 * `onPickPartner` opens the settings to pick a partner once it is unlocked.
 */
export function YourBuddies({
  profile,
  onPickPartner,
}: {
  profile: BuddyProfile;
  onPickPartner: () => void;
}) {
  return (
    <div className="grid gap-x-10 gap-y-5 md:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)]">
      <ul className="divide-y divide-border/60">
        <BuddyRow buddy={profile.lead} role="Lead" />
        {profile.partner ? (
          <BuddyRow buddy={profile.partner} role="Partner" />
        ) : (
          <PartnerSlot profile={profile} onPickPartner={onPickPartner} />
        )}
      </ul>
      <TodayXp
        today={profile.today}
        syncedAt={profile.hoursSyncedAt}
        twoBuddies={profile.partner !== null}
        className="rounded-xl px-4 py-3"
      />
    </div>
  );
}

function BuddyRow({ buddy, role }: { buddy: Buddy; role: "Lead" | "Partner" }) {
  const name = buddySpeciesName(buddy.speciesId);
  const { fraction } = buddyLevelProgress(buddy);

  return (
    <li className="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
      <BuddyPreview speciesId={buddy.speciesId} />
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <p className="min-w-0 truncate text-sm">
            <span className="font-semibold">{name}</span>
            <span className="text-muted-foreground"> · Lv {buddy.level}</span>
          </p>
          <span className="rounded bg-secondary px-1.5 py-0.5 text-[11px] font-semibold text-secondary-foreground">
            {role}
          </span>
          {!buddy.visible && (
            <Tooltip>
              <TooltipTrigger asChild>
                <span
                  tabIndex={0}
                  className="inline-flex items-center gap-1 rounded bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <EyeOffIcon aria-hidden className="size-3" />
                  Hidden
                </span>
              </TooltipTrigger>
              <TooltipContent className="max-w-60 text-xs">
                {name} is not shown on the timer, but still fights in battles.
              </TooltipContent>
            </Tooltip>
          )}
        </div>
        <Progress
          value={fraction * 100}
          aria-label={`${name}'s progress to the next level`}
          className="h-1.5"
        />
        <p className="text-xs tabular-nums text-muted-foreground">
          {buddy.nextLevelXp === null
            ? `Max level · ${formatBuddyXp(buddy.xp)}`
            : `${(buddy.xp - buddy.levelXp).toLocaleString("en-US")} / ${formatBuddyXp(buddy.nextLevelXp - buddy.levelXp)} to Lv ${buddy.level + 1}`}
        </p>
      </div>
    </li>
  );
}

/** The partner's place before there is one: locked, or ready to be picked. */
function PartnerSlot({
  profile,
  onPickPartner,
}: {
  profile: BuddyProfile;
  onPickPartner: () => void;
}) {
  const { lead, partnerUnlockLevel } = profile;
  const leadName = buddySpeciesName(lead.speciesId);
  const locked = lead.level < partnerUnlockLevel;
  const xpToGo = Math.max(0, buddyXpForLevel(partnerUnlockLevel) - lead.xp);

  return (
    <li className="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
      <div
        aria-hidden="true"
        className="flex size-16 shrink-0 items-center justify-center rounded-md border border-dashed border-border/80 bg-background/40 text-muted-foreground"
      >
        {locked ? (
          <LockIcon className="size-5" />
        ) : (
          <PlusIcon className="size-5" />
        )}
      </div>
      <div className="min-w-0 flex-1 space-y-1">
        <p className="text-sm font-semibold">Partner</p>
        {locked ? (
          <p className="text-xs text-muted-foreground">
            Unlocks when {leadName} reaches Lv {partnerUnlockLevel},{" "}
            <span className="tabular-nums">{formatBuddyXp(xpToGo)}</span> from
            now.
          </p>
        ) : (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
            <p className="text-xs text-muted-foreground">
              {leadName} can bring a second buddy to fight next to it.
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 px-2.5 text-xs"
              onClick={onPickPartner}
            >
              Pick a partner
            </Button>
          </div>
        )}
      </div>
    </li>
  );
}
