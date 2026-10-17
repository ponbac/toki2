import {
  type BuddyFacing,
  BuddySprite,
} from "@/components/timer-buddy/buddy-sprite";
import type { BattleType } from "@/lib/api/queries/buddy";
import { BATTLE_XP, buddyXpForLevel } from "@/lib/buddy/level";
import type { BuddyAnimName, BuddySheet } from "@/lib/buddy/sheet";
import {
  BattleScreen,
  SCREEN_WIDTH,
  STANDING,
  TYPE_COLORS,
  menuRound,
} from "./screen";
import {
  type BattleScript,
  type Beat,
  SEATS,
  type Scene,
  type ScriptMon,
  type Seat,
  type Step,
} from "./script";

/** Where the replay is, for its controls. */
export type ReplayStatus = {
  /** The last page is showing and nothing more will happen. */
  readonly ended: boolean;
  /** The page showing when the replay is stepped through, from 0. */
  readonly stop: number;
};

export type BattleDirectorOptions = {
  /** An empty element the screen is built in. */
  readonly root: HTMLElement;
  readonly script: BattleScript;
  /** Every species in the battle, by id. */
  readonly sheets: ReadonlyMap<number, BuddySheet>;
  /**
   * Steps through the text one page at a time, with nothing moving, instead
   * of playing; for reduced motion.
   */
  readonly stepped: boolean;
  /** How fast the replay plays: 1 is normal, 2 double. */
  readonly speed: number;
  readonly onStatus: (status: ReplayStatus) => void;
  /** Each page of text as it appears, in one line, for a live region. */
  readonly onMessage: (text: string) => void;
};

/** The viewer's buddies are seen from behind; the other side faces them. */
const FACING: Readonly<Record<Seat, BuddyFacing>> = { near: "UR", far: "DL" };

/** How far a physical attack lunges, toward the top right for the near buddy. */
const LUNGE = { x: 16, y: -8 };

/** How far a fainting buddy sinks, enough to leave its side's clip. */
const FAINT_DROP = 74;

/**
 * Bodies up to this tall, in sprite pixels, are drawn at double size. Taller
 * ones stay at their native size so they fit on the screen as the games'
 * 64-pixel sprites do.
 */
const DOUBLE_SIZE_MAX_BODY = 31;

/** Game time per character typed into the text box. */
const TYPE_MS = 16;

/** A frame longer than this, after a stall, is not caught up on. */
const MAX_FRAME_MS = 50;

/** How long each kind of text stays up before the replay moves on. */
const HOLD_MS: Readonly<Record<Beat["kind"], number>> = {
  intro: 800,
  send: 0,
  move: 200,
  miss: 700,
  immune: 700,
  hit: 0,
  note: 700,
  faint: 700,
  end: 1000,
  exp: 600,
};

/** Rejects a step of the replay that was cut short by a skip, a replay or unmounting. */
class Cancelled extends Error {
  constructor() {
    super("The battle replay moved on");
  }
}

/** A buddy out on the field. */
type Fighter = {
  readonly seat: Seat;
  readonly index: number;
  readonly mon: ScriptMon;
  readonly sprite: BuddySprite;
  /** Wraps the sprite, for the send-out pop, the hit blink and the silhouette. */
  readonly holder: HTMLDivElement;
  /** Offset from where the buddy stands, in logical pixels. */
  dx: number;
  dy: number;
};

/** A wait in game time; a tween also updates something as it goes. */
type Wait = {
  readonly duration: number;
  elapsed: number;
  readonly update: ((progress: number) => void) | null;
  readonly done: () => void;
};

/** Sprite scale on the logical screen: double, unless the buddy is big. */
function battleScale(sheet: BuddySheet): number {
  const idle = sheet.anims.Idle;
  return idle.foot - idle.top <= DOUBLE_SIZE_MAX_BODY ? 2 : 1;
}

function ignoreCancelled(error: unknown): void {
  if (!(error instanceof Cancelled)) {
    console.error("Battle replay effect failed", error);
  }
}

/**
 * Plays a battle script on a battle screen: the opening, send-outs, the move
 * roulette, attacks, hits and faints, in game time that 2x speed shortens and
 * that stops while the tab is hidden. Skipping or replaying cancels whatever
 * is running and draws a scene straight from the script.
 */
export class BattleDirector {
  private readonly screen: BattleScreen;
  private readonly script: BattleScript;
  private readonly sheets: ReadonlyMap<number, BuddySheet>;
  private readonly stepped: boolean;
  private readonly onStatus: (status: ReplayStatus) => void;
  private readonly onMessage: (text: string) => void;
  private speed: number;

  private readonly fighters: Record<Seat, Fighter | null> = {
    near: null,
    far: null,
  };
  private readonly waits = new Set<Wait>();
  /** Rejects every step in progress. */
  private readonly cancels = new Set<() => void>();
  private frameRequest = 0;
  private lastFrameAt = 0;
  private stop = 0;
  private destroyed = false;

  constructor(options: BattleDirectorOptions) {
    this.screen = new BattleScreen(options.root);
    this.script = options.script;
    this.sheets = options.sheets;
    this.stepped = options.stepped;
    this.speed = options.speed;
    this.onStatus = options.onStatus;
    this.onMessage = options.onMessage;
  }

  /** Starts playing, or shows the first page when stepping. */
  start(): void {
    if (this.stepped) {
      this.showStop(0);
      return;
    }
    this.lastFrameAt = performance.now();
    this.frameRequest = requestAnimationFrame(this.frame);
    this.play();
  }

  /** Starts over from the opening. */
  replay(): void {
    if (this.stepped) {
      this.showStop(0);
    } else {
      this.play();
    }
  }

  /** Jumps to the end of the battle. */
  skip(): void {
    if (this.stepped) {
      this.showStop(this.script.stops.length - 1);
      return;
    }
    this.renderScene(this.script.finalScene, this.script.finalText, false);
    this.onStatus({ ended: true, stop: this.script.stops.length - 1 });
  }

  /** Shows the next page when stepping. */
  next(): void {
    if (this.stepped) {
      this.showStop(this.stop + 1);
    }
  }

  setSpeed(speed: number): void {
    this.speed = speed;
  }

  /** Stops everything and removes the screen. */
  destroy(): void {
    this.destroyed = true;
    this.cancelAll();
    cancelAnimationFrame(this.frameRequest);
    this.removeFighter("near");
    this.removeFighter("far");
    this.screen.destroy();
  }

  private readonly frame = (now: number): void => {
    if (this.destroyed) {
      return;
    }
    const elapsed =
      Math.min(MAX_FRAME_MS, Math.max(0, now - this.lastFrameAt)) * this.speed;
    this.lastFrameAt = now;

    for (const wait of [...this.waits]) {
      wait.elapsed += elapsed;
      const progress =
        wait.duration > 0 ? Math.min(1, wait.elapsed / wait.duration) : 1;
      wait.update?.(progress);
      if (progress >= 1) {
        this.waits.delete(wait);
        wait.done();
      }
    }
    for (const seat of SEATS) {
      this.fighters[seat]?.sprite.tick(elapsed / 1000);
    }

    this.frameRequest = requestAnimationFrame(this.frame);
  };

  private play(): void {
    this.renderScene(this.script.initial, "", false);
    this.screen.setGroupOffset("far", -SCREEN_WIDTH);
    this.screen.setGroupOffset("near", SCREEN_WIDTH);
    this.onStatus({ ended: false, stop: 0 });
    void this.run();
  }

  private async run(): Promise<void> {
    try {
      let previous = this.script.initial;
      for (const step of this.script.steps) {
        await this.perform(step, previous);
        previous = step.scene;
      }
      this.screen.setArrow(false);
      this.onStatus({ ended: true, stop: this.script.stops.length - 1 });
    } catch (error) {
      if (error instanceof Cancelled) {
        return;
      }
      // The battle itself is fine; show how it ended rather than a frozen screen.
      console.error("Battle replay failed; showing its end", error);
      try {
        this.skip();
      } catch (skipError) {
        console.error("Could not show the end of the battle", skipError);
      }
    }
  }

  private async perform(step: Step, previous: Scene): Promise<void> {
    const { beat } = step;
    const hold = HOLD_MS[beat.kind];
    switch (beat.kind) {
      case "intro":
        await this.tween(900, (progress) => {
          const away = SCREEN_WIDTH * (1 - progress) ** 3;
          this.screen.setGroupOffset("far", -away);
          this.screen.setGroupOffset("near", away);
        });
        await this.say(step.pages, hold);
        return;
      case "send":
        await this.say(step.pages, hold);
        await this.sendOut(beat.seat, beat.index, step.scene);
        return;
      case "move":
        await this.useMove(
          step,
          previous,
          beat.seat,
          beat.moveIndex,
          beat.lands,
        );
        return;
      case "hit":
        await this.takeHit(beat.seat, beat.hpBefore, beat.hpAfter, {
          shake: beat.crit || beat.effectiveness > 1,
        });
        return;
      case "faint":
        await this.faint(step, beat.seat);
        return;
      case "exp":
        // The bar fills while the text is up, then the text stays a moment.
        await this.say(step.pages, 0);
        await this.tween(500, (progress) =>
          this.screen.setExp(this.expShare() * progress),
        );
        this.screen.setArrow(true);
        await this.wait(hold);
        return;
      case "miss":
      case "immune":
      case "note":
      case "end":
        await this.say(step.pages, hold);
        return;
    }
  }

  private async sendOut(
    seat: Seat,
    index: number,
    scene: Scene,
  ): Promise<void> {
    const fighter = this.addFighter(seat, index);
    const holder = fighter.holder.style;
    const standing = STANDING[seat];
    holder.transformOrigin = `${standing.x}px ${standing.y}px`;
    holder.filter = "brightness(0) invert(1)";
    await this.tween(260, (progress) => {
      holder.transform = `scale(${progress})`;
    });
    await this.wait(140);
    holder.filter = "";
    holder.transform = "";

    this.screen.showBox(seat, fighter.mon, scene[seat]);
    const from = seat === "near" ? 50 : -50;
    await this.tween(260, (progress) =>
      this.screen.setBoxOffset(seat, from * (1 - progress)),
    );
    await this.wait(350);
  }

  private async useMove(
    step: Step,
    previous: Scene,
    seat: Seat,
    moveIndex: number,
    lands: boolean,
  ): Promise<void> {
    const fighter = this.fighters[seat];
    const move = fighter?.mon.moves[moveIndex];
    if (!fighter || !move) {
      await this.say(step.pages, HOLD_MS.miss);
      return;
    }

    if (seat === "near") {
      await this.roulette(
        fighter.mon,
        previous.near.ppLeft[fighter.index] ?? [],
        moveIndex,
      );
    }
    await this.say(step.pages, HOLD_MS.move);
    if (!lands) {
      return;
    }

    const target: Seat = seat === "near" ? "far" : "near";
    if (move.category === "special") {
      await this.animate(seat, "Charge");
      this.spawn(this.flashType(move.type));
      await this.projectile(seat, target, move.type);
      return;
    }

    const swing = this.animate(seat, "Attack");
    const direction = seat === "near" ? 1 : -1;
    const lunge = (share: number) =>
      this.shift(
        fighter,
        direction * LUNGE.x * share,
        direction * LUNGE.y * share,
      );
    await this.tween(200, lunge);
    await this.wait(100);
    await this.tween(200, (progress) => lunge(1 - progress));
    await swing;
    await this.burst(target, move.type);
  }

  /** The move menu's cursor spins round and slows down on the move that was used. */
  private async roulette(
    mon: ScriptMon,
    ppLeft: readonly number[],
    pick: number,
  ): Promise<void> {
    const round = menuRound(mon.moves.length);
    const stops = round.length + Math.max(0, round.indexOf(pick));
    this.screen.setMessage("");
    this.screen.showMenu(mon.moves);
    for (let stop = 0; stop <= stops; stop += 1) {
      const index = round[stop % round.length] ?? 0;
      const move = mon.moves[index];
      if (move) {
        this.screen.setMenuCursor(index, move, ppLeft[index] ?? move.pp);
      }
      await this.wait(40 + stop * stop * 4);
    }
    this.screen.pickMenuMove(pick);
    await this.wait(380);
    this.screen.hideMenu();
  }

  private async takeHit(
    seat: Seat,
    hpBefore: number,
    hpAfter: number,
    { shake }: { shake: boolean },
  ): Promise<void> {
    const fighter = this.fighters[seat];
    if (!fighter) {
      return;
    }

    const hurt = this.animate(seat, "Hurt", 2);
    if (shake) {
      this.spawn(this.shake());
    }
    for (let blink = 0; blink < 6; blink += 1) {
      fighter.holder.style.visibility = blink % 2 === 0 ? "hidden" : "";
      await this.wait(65);
    }
    fighter.holder.style.visibility = "";

    const maxHp = fighter.mon.maxHp;
    await this.tween(
      Math.max(250, ((hpBefore - hpAfter) / maxHp) * 900),
      (progress) =>
        this.screen.setHp(
          seat,
          hpBefore + (hpAfter - hpBefore) * progress,
          maxHp,
        ),
    );
    await hurt;
  }

  private async faint(step: Step, seat: Seat): Promise<void> {
    const fighter = this.fighters[seat];
    if (fighter) {
      await this.tween(380, (progress) =>
        this.shift(fighter, 0, FAINT_DROP * progress),
      );
      this.removeFighter(seat);
    }
    this.screen.setBalls(seat, step.scene[seat].fainted);
    await this.say(step.pages, HOLD_MS.faint);
    this.screen.hideBox(seat);
  }

  /** Types out each page; `hold` keeps it up with the blinking arrow. */
  private async say(pages: readonly string[], hold: number): Promise<void> {
    for (const page of pages) {
      this.screen.setArrow(false);
      this.onMessage(page.replace(/\n/g, " "));
      await this.tween(page.length * TYPE_MS, (progress) =>
        this.screen.setMessage(
          page.slice(0, Math.ceil(progress * page.length)),
        ),
      );
      if (hold > 0) {
        this.screen.setArrow(true);
        await this.wait(hold);
        this.screen.setArrow(false);
      }
    }
  }

  private async shake(): Promise<void> {
    await this.tween(320, (progress) =>
      this.screen.setShake(Math.sin(progress * 28) * 3 * (1 - progress)),
    );
    this.screen.setShake(0);
  }

  private async flashType(type: BattleType): Promise<void> {
    const color = TYPE_COLORS[type];
    await this.tween(260, (progress) =>
      this.screen.setFlash(color, 0.5 * (1 - progress)),
    );
  }

  /** Particles in the move's type colour fly out from the target. */
  private async burst(seat: Seat, type: BattleType): Promise<void> {
    const [x, y] = this.center(seat);
    const color = TYPE_COLORS[type];
    const particles = Array.from({ length: 6 }, (_, index) => {
      const node = document.createElement("div");
      node.className = "bp-particle";
      node.style.background = color;
      this.screen.fx.append(node);
      return { node, angle: (index / 6) * Math.PI * 2 };
    });
    try {
      await this.tween(260, (progress) => {
        for (const { node, angle } of particles) {
          const px = Math.round(x - 2 + Math.cos(angle) * 18 * progress);
          const py = Math.round(y - 2 + Math.sin(angle) * 14 * progress);
          node.style.transform = `translate(${px}px, ${py}px)`;
          node.style.opacity = String(1 - progress * progress);
        }
      });
    } finally {
      for (const { node } of particles) {
        node.remove();
      }
    }
  }

  /** A glowing ball in the move's type colour arcs from one buddy to the other. */
  private async projectile(
    from: Seat,
    to: Seat,
    type: BattleType,
  ): Promise<void> {
    const [x0, y0] = this.center(from);
    const [x1, y1] = this.center(to);
    const color = TYPE_COLORS[type];
    const node = document.createElement("div");
    node.className = "bp-projectile";
    node.style.background = color;
    node.style.boxShadow = `0 0 0 2px #f8f8f8, 0 0 6px 3px ${color}`;
    this.screen.fx.append(node);
    try {
      await this.tween(300, (progress) => {
        const px = Math.round(x0 + (x1 - x0) * progress - 4);
        const py = Math.round(
          y0 + (y1 - y0) * progress - 18 * Math.sin(progress * Math.PI) - 4,
        );
        node.style.transform = `translate(${px}px, ${py}px)`;
      });
    } finally {
      node.remove();
    }
  }

  /** The middle of a side's buddy, in logical pixels. */
  private center(seat: Seat): [number, number] {
    const standing = STANDING[seat];
    const height = this.fighters[seat]?.sprite.bodyHeight ?? 48;
    return [standing.x, standing.y - height / 2];
  }

  /** EXP bar share of the battle XP for the near buddy's level. */
  private expShare(): number {
    const level = this.fighters.near?.mon.level;
    if (level === undefined || level >= 100) {
      return 0;
    }
    const span = buddyXpForLevel(level + 1) - buddyXpForLevel(level);
    return span > 0 ? Math.min(1, BATTLE_XP / span) : 1;
  }

  private showStop(index: number): void {
    const stops = this.script.stops;
    const stop = stops[Math.max(0, Math.min(index, stops.length - 1))];
    if (!stop) {
      return;
    }
    this.stop = stops.indexOf(stop);
    const last = this.stop === stops.length - 1;
    this.renderScene(stop.scene, stop.text, !last);
    this.onStatus({ ended: last, stop: this.stop });
  }

  /**
   * Draws a scene at rest, straight from the script, and cancels whatever was
   * playing. Buddies already out stay, so stepping does not restart them.
   */
  private renderScene(scene: Scene, text: string, arrow: boolean): void {
    this.cancelAll();
    this.screen.reset();
    for (const seat of SEATS) {
      const index = scene[seat].active;
      const mon = index === null ? undefined : this.script.teams[seat][index];
      if (index === null || !mon) {
        this.removeFighter(seat);
        this.screen.hideBox(seat);
        continue;
      }

      const current = this.fighters[seat];
      const fighter =
        current?.index === index ? current : this.addFighter(seat, index);
      fighter.holder.removeAttribute("style");
      fighter.sprite.play("Idle", { facing: FACING[seat] });
      this.shift(fighter, 0, 0);
      this.screen.showBox(seat, mon, scene[seat]);
    }
    this.screen.setExp(scene.expGained ? this.expShare() : 0);
    this.screen.setMessage(text);
    this.screen.setArrow(arrow);
    this.onMessage(text.replace(/\n/g, " "));
  }

  private addFighter(seat: Seat, index: number): Fighter {
    const mon = this.script.teams[seat][index];
    const sheet = mon ? this.sheets.get(mon.speciesId) : undefined;
    if (!mon || !sheet) {
      throw new Error(`No sprites for the ${seat} buddy ${index}`);
    }

    this.removeFighter(seat);
    const sprite = new BuddySprite(sheet, { scale: battleScale(sheet) });
    sprite.play("Idle", { facing: FACING[seat] });
    const holder = document.createElement("div");
    holder.className = "bp-holder";
    holder.append(sprite.element);
    this.screen.layers[seat].append(holder);

    const fighter: Fighter = { seat, index, mon, sprite, holder, dx: 0, dy: 0 };
    this.fighters[seat] = fighter;
    this.shift(fighter, 0, 0);
    return fighter;
  }

  private removeFighter(seat: Seat): void {
    this.fighters[seat]?.holder.remove();
    this.fighters[seat] = null;
  }

  /** Moves a buddy away from where it stands and redraws it. */
  private shift(fighter: Fighter, dx: number, dy: number): void {
    fighter.dx = dx;
    fighter.dy = dy;
    const standing = STANDING[fighter.seat];
    fighter.sprite.x = standing.x + dx;
    fighter.sprite.y = standing.y + dy;
    fighter.sprite.draw();
  }

  /** Plays a buddy's one-shot animation `times` times, then idles. */
  private animate(seat: Seat, anim: BuddyAnimName, times = 1): Promise<void> {
    const fighter = this.fighters[seat];
    if (!fighter) {
      return Promise.resolve();
    }
    return this.pending((done) => {
      let left = times;
      const once = () =>
        fighter.sprite.play(anim, {
          facing: FACING[seat],
          loop: false,
          onEnd: () => {
            left -= 1;
            if (left > 0) {
              once();
              return;
            }
            fighter.sprite.play("Idle", { facing: FACING[seat] });
            done();
          },
        });
      once();
    });
  }

  private wait(duration: number): Promise<void> {
    return this.tween(duration, null);
  }

  /** Calls `update` with the progress, 0 to 1, over `duration` of game time. */
  private tween(
    duration: number,
    update: ((progress: number) => void) | null,
  ): Promise<void> {
    return this.pending((done) => {
      update?.(0);
      this.waits.add({ duration, elapsed: 0, update, done });
    });
  }

  /**
   * A step that `cancelAll` rejects. Its rejection is always handled, so a
   * step running beside the awaited one never reports an unhandled one.
   */
  private pending(start: (done: () => void) => void): Promise<void> {
    const promise = new Promise<void>((resolve, reject) => {
      const cancel = () => reject(new Cancelled());
      this.cancels.add(cancel);
      start(() => {
        if (this.cancels.delete(cancel)) {
          resolve();
        }
      });
    });
    promise.catch(ignoreCancelled);
    return promise;
  }

  /** Runs a step beside the awaited ones. */
  private spawn(task: Promise<void>): void {
    task.catch(ignoreCancelled);
  }

  private cancelAll(): void {
    this.waits.clear();
    const cancels = [...this.cancels];
    this.cancels.clear();
    for (const cancel of cancels) {
      cancel();
    }
  }
}
