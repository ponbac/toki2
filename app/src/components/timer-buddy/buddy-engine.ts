import { formatBuddyDuration, formatBuddyMinutes } from "@/lib/buddy/format";
import type { BuddyEnergy } from "@/lib/buddy/preferences";
import type { BuddyAnimName, BuddySheet } from "@/lib/buddy/sheet";
import { type BuddyAnchor, BuddyEffects } from "./buddy-effects";
import { type BuddyFacing, BuddySprite } from "./buddy-sprite";

/** Today's logged and scheduled hours, the two ends of the status plate's EXP bar. */
export type BuddyDay = {
  readonly coveredHours: number;
  readonly scheduledHours: number;
};

type BuddyEngineOptions = {
  /** A viewport-sized, fixed-position element the buddy draws into. */
  readonly layer: HTMLElement;
  readonly sheet: BuddySheet;
  /** Shows a still frame on its perch and skips every animation and effect. */
  readonly reducedMotion: boolean;
};

/** What the buddy stands on: the timer card, its minimized pill, or the screen's bottom edge. */
type Ground = {
  readonly kind: "card" | "pill" | "floor";
  readonly x1: number;
  readonly x2: number;
  readonly y: number;
};

/** A timer button the pointer is resting on. */
type Hover =
  | { readonly kind: "save"; readonly x: number }
  | { readonly kind: "delete" };

type Move = readonly [anim: BuddyAnimName, facing?: BuddyFacing];

type Drag = {
  readonly pointerId: number;
  readonly startX: number;
  readonly startY: number;
  held: boolean;
};

const PERCH_SELECTOR = "[data-buddy-perch]";
/** Keeps the buddy off the rounded corners of the card. */
const PERCH_INSET = 20;
/** The card re-mounts briefly around saves; a short absence is not "no timer". */
const PERCH_LOSS_GRACE_S = 0.4;
const MEASURE_INTERVAL_S = 0.25;
const STILL_REFRESH_MS = 500;
const SLEEP_AFTER_MS = 60 * 60 * 1000;
const NAP_BUBBLE_INTERVAL_S = 4;
const GRAVITY = 1500;
const JUMP_S = 0.55;
const JUMP_HEIGHT = 50;
const DRAG_THRESHOLD = 5;
const POKE_WINDOW_MS = 1800;
const SCREEN_MARGIN = 12;
/** Tailwind's `md` breakpoint, where the timer moves from centred to the right corner. */
const WIDE_VIEWPORT = 768;
const TIMER_CARD_WIDTH = 400;
const TIMER_CARD_MARGIN = 16;

const random = (min: number, max: number) => min + Math.random() * (max - min);
const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), Math.max(min, max));

/**
 * The timer buddy: a Pokemon that lives on the floating timer and reacts to
 * what the user does with it.
 *
 * Behaviour is a chain of small continuations. `task` is the one running now,
 * and starting another replaces it, which is how events interrupt idling.
 * `busy` guards the reactions that must play to the end.
 */
export class BuddyEngine {
  private readonly layer: HTMLElement;
  private readonly sheet: BuddySheet;
  private readonly still: boolean;
  private readonly sprite: BuddySprite;
  private readonly effects: BuddyEffects;
  private readonly hit: HTMLDivElement;
  private readonly cleanups: Array<() => void> = [];

  private energy: BuddyEnergy = "calm";
  /** Where the buddy belongs right now, re-measured a few times a second. */
  private home: Ground;
  /** What the buddy is standing on, which lags `home` until it relocates. */
  private ground: Ground;
  private atHome = true;
  private airborne = false;
  private task: ((dt: number) => void) | null = null;
  private busy: false | "event" | "poke" = false;
  private hover: Hover | null = null;
  private noteFocused = false;
  private asleep = false;
  private napClock = 0;
  private side: "L" | "R" = "R";
  private drag: Drag | null = null;
  private pokes = 0;
  private lastPokeAt = 0;
  private lastInteractionAt = Date.now();
  private timerSeconds: number | null = null;
  private day: BuddyDay | null = null;
  private dayAtSave: BuddyDay | null = null;
  private perchMissingFor = 0;
  private dropVelocity = 0;
  private measureClock = 0;
  private lastFrameAt = 0;
  private frameRequest = 0;
  private destroyed = false;

  constructor(options: BuddyEngineOptions) {
    this.layer = options.layer;
    this.sheet = options.sheet;
    this.still = options.reducedMotion;
    this.sprite = new BuddySprite(options.sheet);
    this.effects = new BuddyEffects(options.layer);
    this.hit = document.createElement("div");
    this.hit.className = "tb-hit";

    this.home = this.findPerch() ?? this.homeFloor();
    this.ground = this.home;
    this.sprite.x = (this.home.x1 + this.home.x2) / 2;
    this.sprite.y = this.home.y;
  }

  /** Mounts the buddy and starts it. Call `destroy` to undo everything. */
  start(): void {
    this.layer.append(this.sprite.element);
    this.cleanups.push(() => this.sprite.element.remove());
    this.sprite.draw();

    if (this.still) {
      const refresh = window.setInterval(() => {
        this.home = this.measureHome(STILL_REFRESH_MS / 1000);
        this.ground = this.home;
        this.sprite.x = clamp(this.sprite.x, this.home.x1, this.home.x2);
        this.sprite.y = this.home.y;
        this.sprite.draw();
      }, STILL_REFRESH_MS);
      this.cleanups.push(() => window.clearInterval(refresh));
      return;
    }

    this.layer.append(this.hit);
    this.cleanups.push(() => this.hit.remove());
    this.listen(this.hit, "pointerdown", this.onPointerDown);
    this.listen(this.hit, "pointermove", this.onPointerMove);
    this.listen(this.hit, "pointerup", this.onPointerUp);
    this.listen(this.hit, "pointercancel", this.onPointerUp);
    this.listen(this.hit, "lostpointercapture", this.onPointerUp);

    for (const type of ["pointerdown", "pointermove", "keydown", "wheel"]) {
      this.listen(window, type, this.onInteraction, {
        capture: true,
        passive: true,
      });
    }
    this.listen(document, "visibilitychange", this.onVisibilityChange);

    this.ambient();
    this.scheduleFrame();
  }

  /** Stops the loop, removes every listener and element this engine added. */
  destroy(): void {
    this.destroyed = true;
    cancelAnimationFrame(this.frameRequest);
    this.task = null;
    this.effects.destroy();
    for (const cleanup of this.cleanups.splice(0)) {
      cleanup();
    }
  }

  /** Changes how long the buddy rests between idle behaviours. */
  setEnergy(energy: BuddyEnergy): void {
    this.energy = energy;
  }

  /** Keeps today's totals current so a save can draw the EXP bar. */
  setDay(day: BuddyDay | null): void {
    this.day = day;
  }

  /**
   * Feeds the running timer's elapsed seconds. Ticking past a full hour
   * triggers the hourly spin; jumps such as the first load do not.
   */
  setTimerSeconds(seconds: number | null): void {
    const previous = this.timerSeconds;
    this.timerSeconds = seconds;
    if (previous === null || seconds === null) {
      return;
    }

    const hours = Math.floor(seconds / 3600);
    const tickedOver =
      seconds > previous &&
      seconds - previous < 5 &&
      hours > Math.floor(previous / 3600);
    if (
      tickedOver &&
      this.canReact() &&
      this.ground.kind === "card" &&
      !this.noteFocused
    ) {
      this.busy = "event";
      this.effects.floatText(`${hours}h!`, "gain", this.anchor());
      this.sequence([["Rotate"], ["Hop"]], this.finish);
    }
  }

  /** The pointer is over the save button at viewport x, or left it (`null`). */
  saveHover(x: number | null): void {
    if (x === null) {
      this.clearHover("save");
      return;
    }
    this.setHover({ kind: "save", x });
  }

  /** The pointer entered or left the delete button. */
  deleteHover(active: boolean): void {
    if (active) {
      this.setHover({ kind: "delete" });
    } else {
      this.clearHover("delete");
    }
  }

  /** The note field took focus: step aside and hold still. */
  noteFocus(): void {
    this.noteFocused = true;
    if (this.canReact() && !this.hover) {
      this.settle();
    }
  }

  /** The note field lost focus; `changed` says whether the note was saved. */
  noteBlur(changed: boolean): void {
    if (!this.noteFocused) {
      return;
    }
    this.noteFocused = false;
    // A hovered button keeps its pose: the pointer is about to click it.
    if (!this.canReact() || this.hover) {
      return;
    }

    if (changed) {
      this.busy = "event";
      this.effects.bubble("note", this.anchor());
      this.once("Hop", this.finish);
    } else {
      this.rest();
    }
  }

  /** A save was requested; remembers today's totals from before it lands. */
  saveStarted(): void {
    this.dayAtSave = this.day;
  }

  /**
   * A timer entry of `seconds` was saved. `countsTowardToday` is false for a
   * timer started on an earlier day, whose hours belong to that day's total.
   */
  saved(seconds: number, countsTowardToday: boolean): void {
    const day = countsTowardToday ? this.dayAtSave : null;
    this.dayAtSave = null;
    if (this.still || this.airborne) {
      return;
    }

    this.hover = null;
    this.asleep = false;
    this.busy = "event";

    const goalMinutes = (day?.scheduledHours ?? 0) * 60;
    const fromMinutes = (day?.coveredHours ?? 0) * 60;
    const toMinutes = fromMinutes + seconds / 60;
    const reachedGoal =
      goalMinutes > 0 && fromMinutes < goalMinutes && toMinutes >= goalMinutes;

    if (goalMinutes > 0) {
      this.effects.plate(
        {
          name: this.sheet.name.toUpperCase(),
          tag: `+${seconds < 60 ? formatBuddyDuration(seconds) : formatBuddyMinutes(seconds / 60)}`,
          fromMinutes,
          toMinutes,
          goalMinutes,
        },
        this.anchor(),
      );
    } else {
      this.effects.floatText(
        `+${formatBuddyDuration(seconds)}`,
        "gain",
        this.anchor(),
      );
    }

    if (reachedGoal) {
      this.sequence(
        [["Charge", this.side], ["Double", this.side], ["Hop"], ["Hop"]],
        this.finish,
      );
    } else if (seconds >= 3600) {
      this.sequence([["Rotate"], ["Hop"], ["Hop"]], this.finish);
    } else {
      this.sequence([["Hop"], ["Hop"]], this.finish);
    }
  }

  /** The timer was thrown away after running for `seconds`. */
  deleted(seconds: number): void {
    if (this.still || this.airborne) {
      return;
    }

    this.hover = null;
    this.asleep = false;
    this.busy = "event";
    this.effects.floatText(
      `-${formatBuddyDuration(seconds)}`,
      "loss",
      this.anchor(),
    );
    this.sequence([["Hurt"], ["Hurt"], ["Hurt"]], this.finish);
  }

  // --- frame loop ---------------------------------------------------------

  private scheduleFrame(): void {
    this.lastFrameAt = performance.now();
    this.frameRequest = requestAnimationFrame(this.frame);
  }

  private readonly frame = (now: number): void => {
    if (this.destroyed) {
      return;
    }
    const dt = clamp((now - this.lastFrameAt) / 1000, 0, 0.05);
    this.lastFrameAt = now;

    this.measureClock += dt;
    if (this.measureClock >= MEASURE_INTERVAL_S) {
      this.syncHome(this.measureHome(this.measureClock));
      this.measureClock = 0;
    }

    this.task?.(dt);
    this.dropWithPerch(dt);

    // Checked every frame, not only between idle behaviours: a held pose such
    // as charging over the save button never returns to the idle deck.
    if (
      this.canReact() &&
      Date.now() - this.lastInteractionAt >= SLEEP_AFTER_MS
    ) {
      this.asleep = true;
      this.napClock = 1;
      this.hold("Sleep");
    }

    if (this.asleep) {
      this.napClock -= dt;
      if (this.napClock <= 0) {
        this.effects.bubble("sleep", this.anchor());
        this.napClock = NAP_BUBBLE_INTERVAL_S;
      }
    }

    this.effects.tick(dt);
    this.sprite.tick(dt);
    this.placeHitbox();
    this.frameRequest = requestAnimationFrame(this.frame);
  };

  private readonly onVisibilityChange = (): void => {
    cancelAnimationFrame(this.frameRequest);
    if (!document.hidden && !this.destroyed) {
      this.scheduleFrame();
    }
  };

  // --- where the buddy lives ----------------------------------------------

  private findPerch(): Ground | null {
    const perch = document.querySelector<HTMLElement>(PERCH_SELECTOR);
    if (!perch) {
      return null;
    }

    const rect = perch.getBoundingClientRect();
    const inset = Math.min(PERCH_INSET, rect.width / 4);
    return {
      kind: perch.dataset.buddyPerch === "pill" ? "pill" : "card",
      x1: rect.left + inset,
      x2: rect.right - inset,
      y: Math.round(rect.top),
    };
  }

  /** With no timer the buddy keeps to the corner the timer would occupy. */
  private homeFloor(): Ground {
    const width = window.innerWidth;
    const wide = width >= WIDE_VIEWPORT;
    const left = wide
      ? Math.max(TIMER_CARD_MARGIN, width - TIMER_CARD_MARGIN - TIMER_CARD_WIDTH)
      : width * 0.05;
    const right = wide ? width - TIMER_CARD_MARGIN : width * 0.95;
    return {
      kind: "floor",
      x1: left + PERCH_INSET,
      x2: right - PERCH_INSET,
      y: window.innerHeight,
    };
  }

  private measureHome(dt: number): Ground {
    const perch = this.findPerch();
    if (perch) {
      this.perchMissingFor = 0;
      return perch;
    }

    this.perchMissingFor += dt;
    if (
      this.home.kind !== "floor" &&
      this.perchMissingFor < PERCH_LOSS_GRACE_S
    ) {
      return this.home;
    }
    return this.homeFloor();
  }

  /** The whole bottom edge of the screen, for a buddy dropped away from home. */
  private openFloor(): Ground {
    return {
      kind: "floor",
      x1: SCREEN_MARGIN,
      x2: window.innerWidth - SCREEN_MARGIN,
      y: window.innerHeight,
    };
  }

  private syncHome(next: Ground): void {
    this.home = next;
    if (this.airborne) {
      return;
    }

    if (this.atHome && this.ground.kind === next.kind) {
      this.ground = next;
      this.sprite.x = clamp(this.sprite.x, next.x1, next.x2);
      this.sprite.y = next.y;
      return;
    }
    if (!this.atHome && this.ground.kind === "floor") {
      // Away from home on the open floor: follow the viewport as it resizes.
      this.ground = this.openFloor();
      this.sprite.x = clamp(this.sprite.x, this.ground.x1, this.ground.x2);
      this.sprite.y = this.ground.y;
    }
    if (!this.busy) {
      this.relocate();
    }
  }

  /**
   * The timer went away under a reaction that has to finish, such as the
   * flinch after a delete: fall to the floor without cutting the reaction short.
   */
  private dropWithPerch(dt: number): void {
    if (
      !this.busy ||
      this.airborne ||
      this.ground.kind === "floor" ||
      this.home.kind !== "floor"
    ) {
      this.dropVelocity = 0;
      return;
    }

    this.dropVelocity += GRAVITY * dt;
    this.sprite.y += this.dropVelocity * dt;
    if (this.sprite.y >= window.innerHeight) {
      this.sprite.y = window.innerHeight;
      this.ground = this.openFloor();
      this.atHome = false;
      this.dropVelocity = 0;
    }
  }

  private needsRelocation(): boolean {
    return !this.atHome || this.ground.kind !== this.home.kind;
  }

  /** Walks, drops or jumps to wherever `home` is now. */
  private relocate(): void {
    this.busy = "event";
    this.asleep = false;
    // The buttons and note field unmount with the card and never report leaving.
    this.hover = null;
    this.noteFocused = false;
    const target = this.home;
    const targetX = clamp(this.sprite.x, target.x1, target.x2);
    const speed = Math.max(70, Math.abs(targetX - this.sprite.x) / 3);

    if (target.kind === "floor") {
      const walkHome = () => {
        this.ground = this.openFloor();
        this.atHome = false;
        this.walk(targetX, speed, () => {
          this.ground = target;
          this.atHome = true;
          this.finish();
        });
      };
      if (this.ground.kind === "floor") {
        walkHome();
      } else {
        this.fall(window.innerHeight, walkHome);
      }
      return;
    }

    const jumpUp = () =>
      this.jump(targetX, target.y, () => {
        this.ground = target;
        this.atHome = true;
        this.once("Hop", this.finish);
      });
    if (this.ground.kind === "floor") {
      this.ground = this.openFloor();
      this.atHome = false;
      this.effects.bubble("alert", this.anchor());
      this.idle(0.45, () => this.walk(targetX, speed, jumpUp));
    } else {
      jumpUp();
    }
  }

  // --- idle behaviour -----------------------------------------------------

  private readonly finish = (): void => {
    this.busy = false;
    if (this.needsRelocation()) {
      this.relocate();
    } else {
      this.settle();
    }
  };

  /** Returns to whatever the pointer and note field call for, or to idling. */
  private settle(): void {
    if (this.ground.kind !== "card") {
      this.rest();
    } else if (this.hover) {
      this.reactToHover(this.hover);
    } else if (this.noteFocused) {
      this.walk(this.ground.x1 + 24, 100, () => this.hold("Idle"));
    } else {
      this.rest();
    }
  }

  private readonly rest = (): void => {
    this.idle(random(1.2, 3) * (this.energy === "calm" ? 4 : 1), this.ambient);
  };

  /** Draws the next idle behaviour from a weighted deck. */
  private readonly ambient = (): void => {
    if (this.ground.kind === "pill") {
      // The minimized timer is the quiet mode: stand, and glance around now and then.
      this.idle(random(4, 8), () => this.lookAround(this.ambient));
      return;
    }

    const { x1, x2 } = this.ground;
    const roll = Math.random();
    if (roll < 0.3) {
      this.walk(random(x1, x2), 44, this.rest);
    } else if (roll < 0.48) {
      this.lookAround(this.rest);
    } else if (roll < 0.72) {
      const moves: ReadonlyArray<ReadonlyArray<Move>> = [
        [
          ["Charge", this.side],
          ["Attack", this.side],
        ],
        [["Swing", this.side]],
        [
          ["Charge", this.side],
          ["Double", this.side],
        ],
      ];
      this.sequence(moves[Math.floor(Math.random() * moves.length)] ?? [], this.rest);
    } else if (roll < 0.81) {
      this.once("Rotate", this.rest);
    } else if (roll < 0.9) {
      this.once("Hop", this.rest);
    } else {
      const left = this.sprite.x < (x1 + x2) / 2;
      this.walk(left ? x1 : x2, 44, () =>
        this.idle(2, this.rest, left ? "DL" : "DR"),
      );
    }
  };

  private lookAround(then: () => void): void {
    const glances: BuddyFacing[] = ["L", "D", "R", "D"];
    const next = () => {
      const facing = glances.shift();
      if (facing) {
        this.idle(0.7, next, facing);
      } else {
        then();
      }
    };
    next();
  }

  // --- reactions ----------------------------------------------------------

  private canReact(): boolean {
    return !this.still && !this.busy && !this.airborne && !this.asleep;
  }

  private setHover(hover: Hover): void {
    this.hover = hover;
    if (this.canReact()) {
      this.settle();
    }
  }

  private clearHover(kind: Hover["kind"]): void {
    if (this.hover?.kind !== kind) {
      return;
    }
    this.hover = null;
    if (this.canReact()) {
      this.settle();
    }
  }

  private reactToHover(hover: Hover): void {
    switch (hover.kind) {
      case "save":
        // Run over to the button and power up, so the click lands as a release.
        this.walk(hover.x, 120, () => this.hold("Charge"));
        return;
      case "delete":
        this.walk(this.ground.x1, 130, () => {
          this.hold("Idle", "R");
          this.effects.bubble("alert", this.anchor());
        });
        return;
    }
  }

  private readonly onInteraction = (): void => {
    this.lastInteractionAt = Date.now();
    if (this.asleep && !this.busy && !this.airborne) {
      this.asleep = false;
      this.busy = "event";
      this.effects.bubble("alert", this.anchor());
      this.once("Hop", this.finish);
    }
  };

  private poke(): void {
    if (this.airborne || this.busy === "event") {
      return;
    }

    const now = Date.now();
    this.pokes = now - this.lastPokeAt < POKE_WINDOW_MS ? this.pokes + 1 : 1;
    this.lastPokeAt = now;
    this.busy = "poke";

    if (this.pokes >= 5) {
      this.pokes = 0;
      this.effects.bubble("annoyed", this.anchor());
      this.once("Swing", this.finish, this.side);
    } else if (this.pokes >= 3) {
      this.once("Rotate", this.finish);
    } else {
      this.effects.bubble("heart", this.anchor());
      this.once("Hop", this.finish);
    }
  }

  // --- grab and release ---------------------------------------------------

  private readonly onPointerDown = (event: PointerEvent): void => {
    // One pointer at a time: a second finger must not replace the drag in progress.
    if (event.button !== 0 || this.drag) {
      return;
    }
    this.hit.setPointerCapture(event.pointerId);
    this.drag = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      held: false,
    };
    event.preventDefault();
  };

  private readonly onPointerMove = (event: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || event.pointerId !== drag.pointerId) {
      return;
    }
    if (!drag.held) {
      const distance = Math.hypot(
        event.clientX - drag.startX,
        event.clientY - drag.startY,
      );
      if (distance < DRAG_THRESHOLD) {
        return;
      }
      drag.held = true;
      this.hit.dataset.held = "";
      this.busy = "event";
      this.asleep = false;
      this.airborne = true;
      this.task = null;
      this.sprite.play("Walk", { facing: "D" });
    }

    const bodyHeight = this.sprite.bodyHeight;
    this.sprite.x = clamp(
      event.clientX,
      SCREEN_MARGIN,
      window.innerWidth - SCREEN_MARGIN,
    );
    this.sprite.y = clamp(
      event.clientY + bodyHeight / 2,
      bodyHeight,
      window.innerHeight,
    );
  };

  /** Also runs when capture is lost, so an interrupted drag still lets go. */
  private readonly onPointerUp = (event: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || event.pointerId !== drag.pointerId) {
      return;
    }
    this.drag = null;
    delete this.hit.dataset.held;

    if (!drag.held) {
      this.poke();
      return;
    }

    // Dropped over the timer it lands there; anywhere else it falls to the floor.
    const home = this.home;
    const overPerch =
      home.kind !== "floor" &&
      this.sprite.x >= home.x1 - 10 &&
      this.sprite.x <= home.x2 + 10 &&
      this.sprite.y <= home.y + 6;
    const landing = overPerch ? home : this.openFloor();
    this.fall(landing.y, () => {
      this.ground = landing;
      this.atHome = overPerch;
      this.sprite.x = clamp(this.sprite.x, landing.x1, landing.x2);
      this.once("Hop", this.finish);
    });
  };

  private placeHitbox(): void {
    const height = this.sprite.bodyHeight;
    this.hit.style.height = `${height}px`;
    this.hit.style.transform = `translate(${Math.round(this.sprite.x - 14)}px, ${Math.round(this.sprite.y - height)}px)`;
  }

  // --- movement primitives ------------------------------------------------

  private anchor(): BuddyAnchor {
    return { x: this.sprite.x, headY: this.sprite.y - this.sprite.bodyHeight };
  }

  private idle(seconds: number, then: () => void, facing: BuddyFacing = "D"): void {
    this.sprite.play("Idle", { facing });
    let remaining = seconds;
    this.task = (dt) => {
      remaining -= dt;
      if (remaining <= 0) {
        this.task = null;
        then();
      }
    };
  }

  private walk(x: number, speed: number, then: () => void): void {
    const target = clamp(x, this.ground.x1, this.ground.x2);
    if (Math.abs(target - this.sprite.x) < 2) {
      then();
      return;
    }

    this.side = target > this.sprite.x ? "R" : "L";
    this.sprite.play("Walk", { facing: this.side });
    const direction = this.side === "R" ? 1 : -1;
    this.task = (dt) => {
      // The perch can move or shrink mid-walk; an out-of-range goal is never reached.
      const goal = clamp(target, this.ground.x1, this.ground.x2);
      this.sprite.x += direction * speed * dt;
      if ((goal - this.sprite.x) * direction <= 0) {
        this.sprite.x = goal;
        this.task = null;
        then();
      }
    };
  }

  private once(anim: BuddyAnimName, then: () => void, facing: BuddyFacing = "D"): void {
    this.task = null;
    this.sprite.play(anim, { facing, loop: false, onEnd: then });
  }

  private sequence(moves: ReadonlyArray<Move>, then: () => void): void {
    const queue = [...moves];
    const next = () => {
      const move = queue.shift();
      if (move) {
        this.once(move[0], next, move[1]);
      } else {
        then();
      }
    };
    next();
  }

  private hold(anim: BuddyAnimName, facing: BuddyFacing = "D"): void {
    this.task = null;
    this.sprite.play(anim, { facing });
  }

  private jump(x: number, y: number, then: () => void): void {
    const fromX = this.sprite.x;
    const fromY = this.sprite.y;
    let elapsed = 0;
    this.airborne = true;
    this.side = x >= fromX ? "R" : "L";
    this.sprite.play("Walk", { facing: this.side });
    this.task = (dt) => {
      elapsed += dt;
      const progress = Math.min(1, elapsed / JUMP_S);
      this.sprite.x = fromX + (x - fromX) * progress;
      this.sprite.y =
        fromY +
        (y - fromY) * progress -
        4 * JUMP_HEIGHT * progress * (1 - progress);
      if (progress >= 1) {
        this.airborne = false;
        this.task = null;
        then();
      }
    };
  }

  private fall(y: number, then: () => void): void {
    let velocity = 0;
    this.airborne = true;
    this.sprite.play("Walk", { facing: "D" });
    this.task = (dt) => {
      velocity += GRAVITY * dt;
      this.sprite.y += velocity * dt;
      if (this.sprite.y >= y) {
        this.sprite.y = y;
        this.airborne = false;
        this.task = null;
        then();
      }
    };
  }

  private listen<T extends Event>(
    target: EventTarget,
    type: string,
    listener: (event: T) => void,
    options?: AddEventListenerOptions,
  ): void {
    // SAFETY: each call site pairs `type` with the event class that DOM event dispatches.
    const handler = listener as EventListener;
    target.addEventListener(type, handler, options);
    this.cleanups.push(() => target.removeEventListener(type, handler, options));
  }
}
