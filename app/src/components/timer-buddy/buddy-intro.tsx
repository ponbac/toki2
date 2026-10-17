import { useQuery } from "@tanstack/react-query";
import * as React from "react";
import { toast } from "sonner";
import { apiErrorToast } from "@/lib/api/errors";
import { buddyMutations } from "@/lib/api/mutations/buddy";
import { buddyApiQueries } from "@/lib/api/queries/buddy";
import { userQueries } from "@/lib/api/queries/user";
import {
  forgetLegacyBuddyChoice,
  readLegacyBuddyChoice,
} from "@/lib/buddy/legacy-choice";
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
 * Offers the timer buddies once, in a toast that stays until it is answered.
 * Mount it next to the timer; it renders nothing itself. It only asks users
 * who have no buddy profile yet; either answer creates one, and so does
 * turning the buddies on in their settings.
 */
export function TimerBuddyIntro() {
  const { data: profile } = useQuery(buddyApiQueries.profile());
  // `undefined` while loading or after a failed first load: ask nothing then.
  return profile === null ? <UnansweredIntro /> : null;
}

function UnansweredIntro() {
  // A device that answered the old intro moves its answer over while the
  // profile loads; if that could not happen yet, it is retried on the next
  // load rather than asked again. Read once, when the profile came back empty.
  const [legacy] = React.useState(readLegacyBuddyChoice);
  const { data: me } = useQuery({ ...userQueries.me(), staleTime: Infinity });

  if (legacy.enabled !== null || !me) {
    return null;
  }
  // Without an old pick the buddy follows the avatar, so the toast waits for
  // the seed instead of offering the anonymous fallback Pokemon.
  return (
    <IntroToast
      speciesId={legacy.speciesId ?? defaultBuddySpeciesId(me.email)}
    />
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
  // The new profile closes the toast: `TimerBuddyIntro` stops rendering it.
  const answer = buddyMutations.useUpdateBuddyProfile({
    onSuccess: forgetLegacyBuddyChoice,
    onError: apiErrorToast("Failed to save your answer"),
  });
  const reply = (enabled: boolean) =>
    answer.mutate({ enabled, leadSpeciesId: speciesId });

  return (
    <div className="flex w-full items-start gap-3 p-3">
      <BuddyPreview speciesId={speciesId} />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">Want a timer buddy?</p>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {buddySpeciesName(speciesId)} can live on your timer, level up as you
          log hours and battle your colleagues&apos; buddies. Click your avatar
          {/* The avatar is only in the corner on the wide, side-nav layout. */}
          <span className="hidden md:inline"> in the top left corner</span>
          <span className="md:hidden"> in the top bar</span> to pick another
          Pokemon or turn it off.
        </p>
        <div className="mt-2.5 flex gap-2">
          <Button
            type="button"
            size="sm"
            disabled={answer.isPending}
            onClick={() => reply(true)}
          >
            Turn on
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={answer.isPending}
            onClick={() => reply(false)}
          >
            No thanks
          </Button>
        </div>
      </div>
    </div>
  );
}
