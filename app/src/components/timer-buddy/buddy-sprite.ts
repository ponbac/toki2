import type { BuddyAnim, BuddyAnimName, BuddySheet } from "@/lib/buddy/sheet";

/** Screen pixels per sprite pixel on the timer. An integer keeps the pixel art crisp. */
export const BUDDY_SCALE = 2;

/**
 * Where the buddy looks. `UR`/`UL` are the back view, as your own Pokemon is
 * seen in battle. Left-facing directions mirror the right-facing rows.
 */
export type BuddyFacing = "D" | "DR" | "R" | "UR" | "DL" | "L" | "UL";

/** Options for a new sprite. */
export type BuddySpriteOptions = {
  /** Screen pixels per sprite pixel; keep it an integer. Defaults to `BUDDY_SCALE`. */
  readonly scale?: number;
};

type PackedFacing = keyof BuddyAnim["rows"];

/** The packed row each facing draws, and whether it is that row mirrored. */
const FACING_ROWS: Readonly<
  Record<BuddyFacing, readonly [row: PackedFacing, mirrored: boolean]>
> = {
  D: ["D", false],
  DR: ["DR", false],
  R: ["R", false],
  UR: ["UR", false],
  DL: ["DR", true],
  L: ["R", true],
  UL: ["UR", true],
};

/**
 * Atlas y offset of a facing, in sprite pixels. Not every animation is packed
 * in every direction: a missing back view falls back to the side view, and
 * only to the front when there is no side view either; any other missing
 * facing falls back to the front.
 */
function rowOffset(anim: BuddyAnim, facing: PackedFacing): number {
  const rows = anim.rows;
  if (facing === "UR") {
    return rows.UR ?? rows.R ?? rows.D ?? 0;
  }
  return rows[facing] ?? rows.D ?? rows.R ?? 0;
}

type PlayOptions = {
  readonly facing?: BuddyFacing;
  /** Defaults to looping. A one-shot holds its last frame and calls `onEnd` once. */
  readonly loop?: boolean;
  readonly onEnd?: () => void;
};

/** Game-speed moves are over too quickly to read, so they play slower. */
const ANIM_SPEED: Partial<Record<BuddyAnimName, number>> = {
  Attack: 0.5,
  Swing: 0.5,
  Double: 0.55,
  Rotate: 0.45,
  Charge: 0.5,
  Hurt: 0.45,
};

/**
 * Seconds of standing idle before each idle gesture. Looped back to back, as
 * the sheets are timed, a yawn or a stuck-out tongue repeats every second.
 */
const IDLE_REST_S = { min: 5, max: 20 };

function idleRestTicks(): number {
  const seconds =
    IDLE_REST_S.min + Math.random() * (IDLE_REST_S.max - IDLE_REST_S.min);
  return seconds * 60;
}

/**
 * Draws one Pokemon from its atlas into an absolutely positioned element.
 * `x`/`y` are where the feet stand, relative to the element's offset parent.
 */
export class BuddySprite {
  /** The element to mount. It never receives pointer events. */
  readonly element: HTMLDivElement;
  /** Horizontal centre of the feet, in CSS pixels. */
  x = 0;
  /** Ground line the feet stand on, in CSS pixels. */
  y = 0;

  private readonly sheet: BuddySheet;
  private readonly scale: number;
  private anim: BuddyAnimName = "Idle";
  private facing: BuddyFacing = "D";
  private frame = 0;
  private ticks = 0;
  /**
   * Idle ticks left before the idle gesture plays. It only runs down while
   * idling and survives other animations, so short idles still add up to one.
   */
  private restTicks = idleRestTicks();
  private looping = true;
  private onEnd: (() => void) | null = null;
  private drawnTransform = "";
  private drawnPosition = "";

  constructor(sheet: BuddySheet, options: BuddySpriteOptions = {}) {
    this.sheet = sheet;
    this.scale = options.scale ?? BUDDY_SCALE;
    this.element = document.createElement("div");

    const style = this.element.style;
    style.position = "absolute";
    style.left = "0";
    style.top = "0";
    style.pointerEvents = "none";
    style.imageRendering = "pixelated";
    style.backgroundRepeat = "no-repeat";
    style.backgroundImage = `url("${sheet.atlasUrl}")`;
    style.backgroundSize = `${sheet.atlasWidth * this.scale}px ${sheet.atlasHeight * this.scale}px`;
    this.applyFrameSize();
    this.draw();
  }

  /** The animation currently playing. */
  get animation(): BuddyAnimName {
    return this.anim;
  }

  /** Standing height from feet to the top of the head, in CSS pixels. */
  get bodyHeight(): number {
    const idle = this.sheet.anims.Idle;
    return Math.max(8, idle.foot - idle.top) * this.scale;
  }

  /**
   * Starts an animation. Asking for the looping animation that is already
   * playing only updates the facing, so callers can re-issue it freely.
   */
  play(anim: BuddyAnimName, options: PlayOptions = {}): void {
    const loop = options.loop ?? true;
    if (options.facing) {
      this.facing = options.facing;
    }
    if (this.anim === anim && loop && this.looping) {
      return;
    }

    if (this.restTicks <= 0) {
      // Cut off during an idle gesture: count that as the gesture, or it would
      // start over as soon as the buddy idles again.
      this.restTicks = idleRestTicks();
    }
    this.anim = anim;
    this.frame = 0;
    this.ticks = 0;
    this.looping = loop;
    this.onEnd = options.onEnd ?? null;
    this.applyFrameSize();
  }

  /** Turns the buddy without restarting its animation. */
  face(facing: BuddyFacing): void {
    this.facing = facing;
  }

  /** Advances the animation by `dt` seconds and redraws. */
  tick(dt: number): void {
    const durations = this.sheet.anims[this.anim].durations;
    const elapsed = dt * 60 * (ANIM_SPEED[this.anim] ?? 1);
    const resting = this.anim === "Idle" && this.looping;
    const rested = resting ? Math.min(this.restTicks, elapsed) : 0;
    this.restTicks -= rested;
    this.ticks += elapsed - rested;

    let duration = durations[this.frame] ?? 1;
    while (this.ticks >= duration) {
      this.ticks -= duration;
      this.frame += 1;
      if (this.frame >= durations.length) {
        if (this.looping) {
          this.frame = 0;
          if (this.anim === "Idle") {
            this.ticks = 0;
            this.restTicks = idleRestTicks();
            break;
          }
        } else {
          this.frame = durations.length - 1;
          this.ticks = 0;
          const onEnd = this.onEnd;
          this.onEnd = null;
          onEnd?.();
          break;
        }
      }
      duration = durations[this.frame] ?? 1;
    }

    this.draw();
  }

  /** Writes the current frame and position to the element. */
  draw(): void {
    const anim = this.sheet.anims[this.anim];
    const [row, mirrored] = FACING_ROWS[this.facing];
    const rowY = rowOffset(anim, row);

    const position = `${-this.frame * anim.w * this.scale}px ${-rowY * this.scale}px`;
    if (position !== this.drawnPosition) {
      this.element.style.backgroundPosition = position;
      this.drawnPosition = position;
    }

    const left = Math.round(this.x - (anim.w * this.scale) / 2);
    const top = Math.round(this.y - anim.foot * this.scale);
    const transform = `translate(${left}px, ${top}px)${mirrored ? " scaleX(-1)" : ""}`;
    if (transform !== this.drawnTransform) {
      this.element.style.transform = transform;
      this.drawnTransform = transform;
    }
  }

  private applyFrameSize(): void {
    const anim = this.sheet.anims[this.anim];
    this.element.style.width = `${anim.w * this.scale}px`;
    this.element.style.height = `${anim.h * this.scale}px`;
  }
}
