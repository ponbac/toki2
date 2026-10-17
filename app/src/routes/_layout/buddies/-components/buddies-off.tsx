import { BuddyPreview } from "@/components/timer-buddy/buddy-preview";
import { Button } from "@/components/ui/button";
import type { BuddyProfile } from "@/lib/api/queries/buddy";
import { useStartingBuddy } from "@/lib/buddy/profile";
import { buddySpeciesName } from "@/lib/buddy/species";

/**
 * What the Buddies page shows before the user enrols or while their buddies
 * are off: who they would battle with, and a way into the settings.
 * `profile` is `null` before enrolling.
 */
export function BuddiesOff({
  profile,
  onOpenSettings,
}: {
  profile: BuddyProfile | null;
  onOpenSettings: () => void;
}) {
  const starting = useStartingBuddy();
  const speciesId = profile ? profile.lead.speciesId : starting.speciesId;

  return (
    <div className="flex flex-col items-center gap-4 rounded-xl border border-dashed border-border/80 px-6 py-10 text-center sm:flex-row sm:text-left">
      {speciesId !== null && <BuddyPreview speciesId={speciesId} />}
      <div className="min-w-0 flex-1 space-y-1">
        <h3 className="font-display text-lg font-semibold">
          {profile ? "Your buddies are turned off" : "Meet your timer buddy"}
        </h3>
        <p className="max-w-xl text-sm text-muted-foreground">
          {profile
            ? `Turn them on to bring ${buddySpeciesName(profile.lead.speciesId)} back to your timer, battle your colleagues' buddies and fight the daily trainer.`
            : speciesId !== null
              ? `${buddySpeciesName(speciesId)}, ${starting.picked ? "the Pokemon you picked" : "your avatar's Pokemon"}, can live on your timer, level up as you log hours, and battle your colleagues' buddies.`
              : "A Pokemon that lives on your timer, levels up as you log hours, and battles your colleagues' buddies."}
        </p>
      </div>
      <Button type="button" className="shrink-0" onClick={onOpenSettings}>
        {profile ? "Turn them on" : "Get a buddy"}
      </Button>
    </div>
  );
}
