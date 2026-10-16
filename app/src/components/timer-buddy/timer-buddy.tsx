import "@fontsource/press-start-2p/latin-400.css";
import "./timer-buddy.css";
import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import { useAtomValue } from "jotai/react";
import * as React from "react";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { useTimeTrackingTimer } from "@/hooks/useTimeTrackingStore";
import { buddyApiQueries } from "@/lib/api/queries/buddy";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { type BuddyEvent, subscribeBuddyEvents } from "@/lib/buddy/events";
import { timerBuddyEnergyAtom } from "@/lib/buddy/preferences";
import { buddyQueries } from "@/lib/buddy/sheet";
import type { BuddySpeciesId } from "@/lib/buddy/species";
import { type BuddyLane, type BuddyRole, BuddyEngine } from "./buddy-engine";

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
 * The user's buddies on the floating timer: the lead, and the partner when it
 * is shown. Mount it once, next to the timer, while the buddies are enabled;
 * they find the timer through `data-buddy-perch` and hear about saves,
 * deletes and hovers through the buddy event channel.
 */
export default function TimerBuddy() {
  const { data: profile } = useQuery(buddyApiQueries.profile());
  if (!profile?.enabled) {
    return null;
  }

  const partner = profile.partner?.visible ? profile.partner : null;
  return (
    <>
      {/* The lead's layer comes last, so it is on top where the two meet. */}
      {partner && (
        <BuddyStage
          key="partner"
          speciesId={partner.speciesId}
          lane="left"
          role="partner"
        />
      )}
      <BuddyStage
        key="lead"
        speciesId={profile.lead.speciesId}
        lane={partner ? "right" : "full"}
        role="lead"
      />
    </>
  );
}

/** One buddy with its own engine and layer, so its effects never touch the other's. */
function BuddyStage({
  speciesId,
  lane,
  role,
}: {
  speciesId: BuddySpeciesId;
  lane: BuddyLane;
  role: BuddyRole;
}) {
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
  // A new lane moves the running buddy over; it does not start a new one.
  const currentLane = React.useEffectEvent(() => lane);

  React.useEffect(() => {
    const layer = layerRef.current;
    if (!layer || !sheet) {
      return;
    }

    const next = new BuddyEngine({
      layer,
      sheet,
      reducedMotion,
      lane: currentLane(),
      role,
    });
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
  }, [sheet, reducedMotion, role]);

  React.useEffect(() => {
    engine?.setLane(lane);
  }, [engine, lane]);

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
