import { useAtom } from "jotai/react";
import { RotateCcwIcon, ShuffleIcon } from "lucide-react";
import {
  type BuddyEnergy,
  timerBuddyEnabledAtom,
  timerBuddyEnergyAtom,
  timerBuddySpeciesAtom,
} from "@/lib/buddy/preferences";
import { BUDDY_CREDITS_URL } from "@/lib/buddy/sheet";
import {
  BUDDY_SPECIES_LIST,
  parseBuddySpeciesId,
  randomBuddySpeciesId,
  resolveBuddySpeciesId,
} from "@/lib/buddy/species";
import { Combobox } from "../combobox";
import { Button } from "../ui/button";
import { Label } from "../ui/label";
import { Switch } from "../ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import { BuddyPreview } from "./buddy-preview";

const SPECIES_ITEMS = BUDDY_SPECIES_LIST.map((species) => ({
  value: String(species.id),
  label: `${species.name} #${species.id}`,
}));

const ENERGY_OPTIONS: ReadonlyArray<{ value: BuddyEnergy; label: string }> = [
  { value: "calm", label: "Calm" },
  { value: "lively", label: "Lively" },
];

/**
 * Account-settings section for the timer buddy: the on/off switch, which
 * Pokemon it is, and how restless it is. Choices are stored in localStorage.
 *
 * `avatarSeed` picks the default Pokemon, matching the user's fallback avatar.
 */
export function TimerBuddySettings({
  avatarSeed,
}: {
  avatarSeed: string | null | undefined;
}) {
  const [enabled, setEnabled] = useAtom(timerBuddyEnabledAtom);
  const [selectedSpecies, setSelectedSpecies] = useAtom(timerBuddySpeciesAtom);
  const [energy, setEnergy] = useAtom(timerBuddyEnergyAtom);
  const speciesId = resolveBuddySpeciesId(selectedSpecies, avatarSeed);

  return (
    <div className="rounded-md border border-border/60 bg-muted/30 px-3 py-2">
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-0.5">
          <Label
            htmlFor="timer-buddy"
            className="text-sm font-medium text-foreground"
          >
            Timer buddy
          </Label>
          <p className="text-xs text-muted-foreground">
            A Pokemon that lives on your timer and reacts to what you do.
          </p>
        </div>
        <Switch id="timer-buddy" checked={enabled} onCheckedChange={setEnabled} />
      </div>

      {enabled && (
        <div className="mt-3 flex items-center gap-3">
          <BuddyPreview speciesId={speciesId} />
          <div className="min-w-0 flex-1 space-y-2">
            <div className="flex items-center gap-1.5">
              <div className="min-w-0 flex-1">
                <Combobox
                  items={SPECIES_ITEMS}
                  placeholder="Pokemon"
                  searchPlaceholder="Search by name or number..."
                  emptyMessage="No Pokemon found"
                  value={String(speciesId)}
                  onChange={(value) => {
                    // The combobox reports "" when the current item is picked again.
                    const picked = parseBuddySpeciesId(Number(value));
                    if (value !== "" && picked !== null) {
                      setSelectedSpecies(picked);
                    }
                  }}
                />
              </div>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    className="shrink-0"
                    onClick={() => setSelectedSpecies(randomBuddySpeciesId())}
                  >
                    <ShuffleIcon className="size-4" />
                    <span className="sr-only">Random Pokemon</span>
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Random Pokemon</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    className="shrink-0"
                    disabled={selectedSpecies === null}
                    onClick={() => setSelectedSpecies(null)}
                  >
                    <RotateCcwIcon className="size-4" />
                    <span className="sr-only">Back to my default</span>
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Back to my default</TooltipContent>
              </Tooltip>
            </div>
            <div className="flex items-center justify-between gap-2">
              <div
                role="group"
                aria-label="Energy"
                className="flex rounded-md border border-border/60 p-0.5"
              >
                {ENERGY_OPTIONS.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    aria-pressed={energy === option.value}
                    onClick={() => setEnergy(option.value)}
                    className="rounded px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring aria-pressed:bg-accent aria-pressed:text-foreground"
                  >
                    {option.label}
                  </button>
                ))}
              </div>
              <a
                href={BUDDY_CREDITS_URL}
                target="_blank"
                rel="noreferrer"
                className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
              >
                Sprite credits
              </a>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
