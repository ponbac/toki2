import { useQuery } from "@tanstack/react-query";
import { useAtomValue, useSetAtom } from "jotai/react";
import * as React from "react";
import { toast } from "sonner";
import { userQueries } from "@/lib/api/queries/user";
import {
  timerBuddyAnsweredAtom,
  timerBuddyEnabledAtom,
  timerBuddySpeciesAtom,
} from "@/lib/buddy/preferences";
import { buddyQueries } from "@/lib/buddy/sheet";
import {
  type BuddySpeciesId,
  buddySpeciesName,
  defaultBuddySpeciesId,
} from "@/lib/buddy/species";
import { Button } from "../ui/button";
import { BuddyPreview } from "./buddy-preview";

// The card's surface goes on sonner's own toast element, in sonner's colours so
// it matches the toasts it stacks with. sonner shrinks a toast that sits behind
// a newer one, so the surface has to clip and hide its content there, the way
// sonner's built-in toasts do.
const INTRO_TOAST_CLASS =
  "overflow-hidden rounded-lg border border-[var(--normal-border)] bg-[var(--normal-bg)] text-[var(--normal-text)] shadow-[0_4px_12px_rgba(0,0,0,0.1)] [&>*]:transition-opacity [&[data-expanded=false][data-front=false]>*]:opacity-0";

/**
 * Offers the timer buddy once, in a toast that stays until it is answered.
 * Mount it next to the timer; it renders nothing itself. Either answer, or
 * flipping the switch in the account settings, stops it from coming back.
 */
export function TimerBuddyIntro() {
  const answered = useAtomValue(timerBuddyAnsweredAtom);
  const selectedSpecies = useAtomValue(timerBuddySpeciesAtom);
  const { data: me } = useQuery({
    ...userQueries.me(),
    staleTime: Infinity,
    enabled: !answered,
  });

  const speciesId =
    selectedSpecies ?? (me ? defaultBuddySpeciesId(me.email) : null);
  return answered || speciesId === null ? null : (
    <IntroToast speciesId={speciesId} />
  );
}

function IntroToast({ speciesId }: { speciesId: BuddySpeciesId }) {
  // Wait for the sprites, so the toast never opens around an empty tile and
  // never offers a buddy that failed to load.
  const { isSuccess: spritesReady } = useQuery(buddyQueries.sheet(speciesId));

  React.useEffect(() => {
    if (!spritesReady) {
      return;
    }

    const toastId = toast.custom(() => <IntroCard speciesId={speciesId} />, {
      duration: Infinity,
      dismissible: false,
      className: INTRO_TOAST_CLASS,
    });
    return () => {
      toast.dismiss(toastId);
    };
  }, [speciesId, spritesReady]);

  return null;
}

function IntroCard({ speciesId }: { speciesId: BuddySpeciesId }) {
  const setEnabled = useSetAtom(timerBuddyEnabledAtom);

  return (
    <div className="flex w-full items-start gap-3 p-3">
      <BuddyPreview speciesId={speciesId} />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">Want a timer buddy?</p>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {buddySpeciesName(speciesId)} can live on your timer and react when
          you save time. Click your avatar to pick another Pokemon or turn it
          off.
        </p>
        <div className="mt-2.5 flex gap-2">
          <Button type="button" size="sm" onClick={() => setEnabled(true)}>
            Turn on
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={() => setEnabled(false)}
          >
            No thanks
          </Button>
        </div>
      </div>
    </div>
  );
}
