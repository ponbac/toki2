import "@fontsource/press-start-2p/latin-400.css";
import "./timer-buddy.css";
import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import { useAtomValue } from "jotai/react";
import * as React from "react";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { userQueries } from "@/lib/api/queries/user";
import { type BuddyEvent, subscribeBuddyEvents } from "@/lib/buddy/events";
import {
  timerBuddyEnergyAtom,
  timerBuddySpeciesAtom,
} from "@/lib/buddy/preferences";
import { buddyQueries } from "@/lib/buddy/sheet";
import {
  type BuddySpeciesId,
  defaultBuddySpeciesId,
} from "@/lib/buddy/species";
import { BuddyEngine } from "./buddy-engine";

function forwardEvent(engine: BuddyEngine, event: BuddyEvent): void {
  switch (event.type) {
    case "saveHover":
      engine.saveHover(event.x);
      return;
    case "deleteHover":
      engine.deleteHover(event.active);
      return;
    case "noteFocus":
      engine.noteFocus();
      return;
    case "noteBlur":
      engine.noteBlur(event.changed);
      return;
    case "saveStarted":
      engine.saveStarted();
      return;
    case "saved":
      engine.saved(event.seconds, event.countsTowardToday);
      return;
    case "deleted":
      engine.deleted(event.seconds);
      return;
  }
}

/**
 * The Pokemon that lives on the floating timer. Mount it once, next to the
 * timer; it finds the timer through `data-buddy-perch` and hears about saves,
 * deletes and hovers through the buddy event channel.
 */
export default function TimerBuddy() {
  const selectedSpecies = useAtomValue(timerBuddySpeciesAtom);
  const { data: me } = useQuery({ ...userQueries.me(), staleTime: Infinity });

  // Without a pick the buddy follows the avatar, so wait for the seed instead
  // of flashing the anonymous fallback Pokemon first.
  const speciesId =
    selectedSpecies ?? (me ? defaultBuddySpeciesId(me.email) : null);
  return speciesId === null ? null : <BuddyStage speciesId={speciesId} />;
}

function BuddyStage({ speciesId }: { speciesId: BuddySpeciesId }) {
  const energy = useAtomValue(timerBuddyEnergyAtom);
  const reducedMotion = useMediaQuery("(prefers-reduced-motion: reduce)");
  const { timeSeconds } = useTimeTrackingTimer();
  const { data: sheet } = useQuery(buddyQueries.sheet(speciesId));

  const today = dayjs().format("YYYY-MM-DD");
  const { data: day } = useQuery({
    ...timeTrackingQueries.timeInfo({ from: today, to: today }),
    staleTime: 60 * 1000,
  });

  const layerRef = React.useRef<HTMLDivElement>(null);
  const [engine, setEngine] = React.useState<BuddyEngine | null>(null);

  React.useEffect(() => {
    const layer = layerRef.current;
    if (!layer || !sheet) {
      return;
    }

    const next = new BuddyEngine({ layer, sheet, reducedMotion });
    next.start();
    setEngine(next);
    const unsubscribe = subscribeBuddyEvents((event) =>
      forwardEvent(next, event),
    );

    return () => {
      unsubscribe();
      next.destroy();
      setEngine(null);
    };
  }, [sheet, reducedMotion]);

  React.useEffect(() => {
    engine?.setEnergy(energy);
  }, [engine, energy]);

  React.useEffect(() => {
    engine?.setTimerSeconds(timeSeconds);
  }, [engine, timeSeconds]);

  React.useEffect(() => {
    engine?.setDay(
      day
        ? { coveredHours: day.coveredHours, scheduledHours: day.scheduledHours }
        : null,
    );
  }, [engine, day]);

  return <div ref={layerRef} className="tb-layer" aria-hidden="true" />;
}
