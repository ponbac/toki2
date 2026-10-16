import { useQuery } from "@tanstack/react-query";
import * as React from "react";
import { BuddySprite } from "@/components/timer-buddy/buddy-sprite";
import type { BuddyTeamMember } from "@/lib/api/queries/buddy";
import { formatBuddyTeam } from "@/lib/buddy/profile";
import { buddyQueries } from "@/lib/buddy/sheet";
import type { BuddySpeciesId } from "@/lib/buddy/species";
import { cn } from "@/lib/utils";

/**
 * A still buddy in a small tile, facing the viewer. Lists show many of these,
 * so they draw one frame and never animate.
 */
export function BuddyIcon({
  speciesId,
  size = 34,
  className,
}: {
  speciesId: BuddySpeciesId;
  size?: number;
  className?: string;
}) {
  const { data: sheet } = useQuery(buddyQueries.sheet(speciesId));
  const tileRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    const tile = tileRef.current;
    if (!tile || !sheet) {
      return;
    }

    // Native size keeps every pixel crisp; the body is centred, so a big
    // buddy is cropped evenly at its head and feet.
    const sprite = new BuddySprite(sheet, { scale: 1 });
    sprite.x = size / 2;
    sprite.y = Math.round((size + sprite.bodyHeight) / 2);
    sprite.draw();
    tile.append(sprite.element);

    return () => {
      sprite.element.remove();
    };
  }, [sheet, size]);

  return (
    <div
      ref={tileRef}
      aria-hidden="true"
      style={{ width: size, height: size }}
      className={cn(
        "relative shrink-0 overflow-hidden rounded-md bg-muted",
        className,
      )}
    />
  );
}

/**
 * A team's buddies side by side, slightly overlapping, with the team in words
 * for screen readers.
 */
export function TeamIcons({
  team,
  size = 34,
  className,
}: {
  team: readonly BuddyTeamMember[];
  size?: number;
  className?: string;
}) {
  return (
    <span className={cn("flex shrink-0", className)}>
      {team.map((member, index) => (
        <BuddyIcon
          // A team may field the same species twice.
          key={`${index}-${member.speciesId}`}
          speciesId={member.speciesId}
          size={size}
          className={cn(index > 0 && "-ml-1.5 ring-2 ring-card")}
        />
      ))}
      <span className="sr-only">{formatBuddyTeam(team)}</span>
    </span>
  );
}
