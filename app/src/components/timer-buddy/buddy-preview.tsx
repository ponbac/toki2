import { useQuery } from "@tanstack/react-query";
import * as React from "react";
import { useMediaQuery } from "@/hooks/useMediaQuery";
import { buddyQueries } from "@/lib/buddy/sheet";
import type { BuddySpeciesId } from "@/lib/buddy/species";
import { cn } from "@/lib/utils";
import { BuddySprite } from "./buddy-sprite";

const PREVIEW_SIZE = 64;

/** A small tile showing one buddy idling, for the settings dialog. */
export function BuddyPreview({
  speciesId,
  className,
}: {
  speciesId: BuddySpeciesId;
  className?: string;
}) {
  const { data: sheet } = useQuery(buddyQueries.sheet(speciesId));
  const reducedMotion = useMediaQuery("(prefers-reduced-motion: reduce)");
  const tileRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    const tile = tileRef.current;
    if (!tile || !sheet) {
      return;
    }

    const sprite = new BuddySprite(sheet);
    sprite.x = PREVIEW_SIZE / 2;
    sprite.y = PREVIEW_SIZE - 8;
    sprite.draw();
    tile.append(sprite.element);

    let frameRequest = 0;
    if (!reducedMotion) {
      let lastFrameAt = performance.now();
      const frame = (now: number) => {
        sprite.tick(Math.min(0.05, (now - lastFrameAt) / 1000));
        lastFrameAt = now;
        frameRequest = requestAnimationFrame(frame);
      };
      frameRequest = requestAnimationFrame(frame);
    }

    return () => {
      cancelAnimationFrame(frameRequest);
      sprite.element.remove();
    };
  }, [sheet, reducedMotion]);

  return (
    <div
      ref={tileRef}
      aria-hidden="true"
      style={{ width: PREVIEW_SIZE, height: PREVIEW_SIZE }}
      className={cn(
        "relative shrink-0 overflow-hidden rounded-md border border-border/60 bg-background",
        className,
      )}
    />
  );
}
