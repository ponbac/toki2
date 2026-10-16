import type { BattleType } from "@/lib/api/queries/buddy";
import type { ScriptMon, ScriptMove, Seat, SeatScene } from "./script";

/** Logical size of the battle screen, the Game Boy Advance's. */
export const SCREEN_WIDTH = 240;
export const SCREEN_HEIGHT = 160;

/** Where each side's buddy stands: the centre of its feet, in logical pixels. */
export const STANDING: Readonly<Record<Seat, { x: number; y: number }>> = {
  near: { x: 64, y: 110 },
  far: { x: 176, y: 62 },
};

/** The classic type colours, for move effects. */
export const TYPE_COLORS: Readonly<Record<BattleType, string>> = {
  Normal: "#a8a878",
  Fire: "#f08030",
  Water: "#6890f0",
  Electric: "#f8d030",
  Grass: "#78c850",
  Ice: "#98d8d8",
  Fighting: "#c03028",
  Poison: "#a040a0",
  Ground: "#e0c068",
  Flying: "#a890f0",
  Psychic: "#f85888",
  Bug: "#a8b820",
  Rock: "#b8a038",
  Ghost: "#705898",
  Dragon: "#7038f8",
  Dark: "#705848",
  Steel: "#b8b8d0",
};

/** The order the move menu's cursor goes round in: top left, top right, bottom right, bottom left. */
const MENU_ROUND = [0, 1, 3, 2] as const;

function element(className: string, parent?: HTMLElement): HTMLDivElement {
  const node = document.createElement("div");
  node.className = className;
  parent?.append(node);
  return node;
}

function setFlag(node: HTMLElement, name: string, on: boolean): void {
  node.toggleAttribute(`data-${name}`, on);
}

type HpBox = {
  readonly root: HTMLDivElement;
  readonly name: HTMLDivElement;
  readonly balls: HTMLDivElement;
  readonly level: HTMLDivElement;
  readonly hpFill: HTMLDivElement;
  readonly numbers: HTMLDivElement | null;
  readonly expFill: HTMLDivElement | null;
};

/**
 * The battle screen's elements, built inside `root`, and the ways the replay
 * changes them. Holds no battle state of its own.
 */
export class BattleScreen {
  readonly stage: HTMLDivElement;
  readonly layers: Readonly<Record<Seat, HTMLDivElement>>;
  readonly fx: HTMLDivElement;

  private readonly root: HTMLElement;
  private readonly groups: Readonly<Record<Seat, HTMLDivElement>>;
  private readonly flash: HTMLDivElement;
  private readonly boxes: Readonly<Record<Seat, HpBox>>;
  private readonly message: HTMLDivElement;
  private readonly arrow: HTMLDivElement;
  private readonly menu: HTMLDivElement;
  private readonly menuMoves: readonly HTMLDivElement[];
  private readonly menuPp: HTMLDivElement;
  private readonly menuType: HTMLDivElement;

  constructor(root: HTMLElement) {
    this.root = root;
    this.stage = element("bp-stage", root);

    const group = (seat: Seat) => {
      const node = element("bp-group", this.stage);
      element("bp-platform", node).dataset.seat = seat;
      const layer = element("bp-layer", node);
      layer.dataset.seat = seat;
      return { node, layer };
    };
    // The near side is closer, so it is drawn over the far side.
    const far = group("far");
    const near = group("near");
    this.groups = { far: far.node, near: near.node };
    this.layers = { far: far.layer, near: near.layer };

    this.fx = element("bp-fx", this.stage);
    this.flash = element("bp-flash", this.fx);
    this.boxes = { far: this.hpBox("far"), near: this.hpBox("near") };

    const text = element("bp-text", root);
    const frame = element("bp-text-frame", text);
    this.message = element("bp-text-message", frame);
    this.arrow = element("bp-arrow", text);

    this.menu = element("bp-menu", root);
    const moves = element("bp-menu-box bp-menu-moves", this.menu);
    this.menuMoves = [0, 1, 2, 3].map(() => element("bp-menu-move", moves));
    const info = element("bp-menu-box bp-menu-info", this.menu);
    this.menuPp = element("", info);
    this.menuType = element("", info);
    element("bp-menu-random", info).textContent = "RANDOM!";
  }

  /** Removes everything the screen built. */
  destroy(): void {
    this.root.replaceChildren();
  }

  /** Clears effects, shakes, menus and slides, as at the start of a scene. */
  reset(): void {
    for (const node of this.fx.querySelectorAll(
      ".bp-particle, .bp-projectile",
    )) {
      node.remove();
    }
    this.flash.style.opacity = "0";
    this.stage.style.transform = "";
    this.setGroupOffset("far", 0);
    this.setGroupOffset("near", 0);
    this.setBoxOffset("far", 0);
    this.setBoxOffset("near", 0);
    this.hideMenu();
  }

  /** Slides a side's platform and buddy sideways, for the opening. */
  setGroupOffset(seat: Seat, x: number): void {
    this.groups[seat].style.transform =
      x === 0 ? "" : `translateX(${Math.round(x)}px)`;
  }

  setShake(x: number): void {
    this.stage.style.transform =
      x === 0 ? "" : `translateX(${Math.round(x)}px)`;
  }

  setFlash(color: string, opacity: number): void {
    this.flash.style.background = color;
    this.flash.style.opacity = String(opacity);
  }

  /** Fills a side's HP box with a buddy and shows it. */
  showBox(seat: Seat, mon: ScriptMon, scene: SeatScene): void {
    const box = this.boxes[seat];
    const index = scene.active;
    box.name.textContent = mon.name;
    box.level.textContent = `Lv${mon.level}`;
    this.setBalls(seat, scene.fainted);
    this.setHp(seat, index === null ? 0 : (scene.hp[index] ?? 0), mon.maxHp);
    setFlag(box.root, "shown", true);
  }

  hideBox(seat: Seat): void {
    setFlag(this.boxes[seat].root, "shown", false);
  }

  setBoxOffset(seat: Seat, x: number): void {
    this.boxes[seat].root.style.transform =
      x === 0 ? "" : `translateX(${Math.round(x)}px)`;
  }

  /** One ball per team member, greyed out once it has fainted. */
  setBalls(seat: Seat, fainted: readonly boolean[]): void {
    const balls = this.boxes[seat].balls;
    balls.replaceChildren(
      ...fainted.map((out) => {
        const ball = element("bp-ball");
        setFlag(ball, "fainted", out);
        return ball;
      }),
    );
  }

  /** Sets the HP bar, and the HP numbers on the near box. */
  setHp(seat: Seat, hp: number, maxHp: number): void {
    const box = this.boxes[seat];
    const share = maxHp > 0 ? Math.min(1, Math.max(0, hp / maxHp)) : 0;
    box.hpFill.style.width = `${share * 100}%`;
    box.hpFill.dataset.level =
      share > 0.5 ? "high" : share > 0.2 ? "mid" : "low";
    if (box.numbers) {
      const shown = Math.max(0, Math.ceil(hp));
      box.numbers.textContent = `${String(shown).padStart(3)}/${String(maxHp).padStart(3)}`;
    }
  }

  /** Fills the near box's EXP bar; anything gained shows at least a sliver. */
  setExp(share: number): void {
    const fill = this.boxes.near.expFill;
    if (fill) {
      fill.style.width =
        share > 0 ? `max(2px, ${Math.min(1, share) * 100}%)` : "0";
    }
  }

  setMessage(text: string): void {
    this.message.textContent = text;
  }

  setArrow(shown: boolean): void {
    setFlag(this.arrow, "shown", shown);
  }

  /** Shows a buddy's moves over the text box, with no cursor yet. */
  showMenu(moves: readonly ScriptMove[]): void {
    this.menuMoves.forEach((node, index) => {
      node.textContent = moves[index]?.name ?? "-";
      setFlag(node, "cursor", false);
      setFlag(node, "picked", false);
    });
    this.menuPp.textContent = "";
    this.menuType.textContent = "";
    setFlag(this.menu, "shown", true);
  }

  /** Puts the cursor on a move and shows its PP and type. */
  setMenuCursor(index: number, move: ScriptMove, ppLeft: number): void {
    this.menuMoves.forEach((node, other) =>
      setFlag(node, "cursor", other === index),
    );
    this.menuPp.textContent = `PP ${ppLeft}/${move.pp}`;
    this.menuType.textContent = `TYPE/${move.type.toUpperCase()}`;
  }

  /** Marks the move the roulette landed on. */
  pickMenuMove(index: number): void {
    const node = this.menuMoves[index];
    if (node) {
      setFlag(node, "picked", true);
    }
  }

  hideMenu(): void {
    setFlag(this.menu, "shown", false);
  }

  private hpBox(seat: Seat): HpBox {
    const root = element("bp-box", this.stage);
    root.dataset.seat = seat;
    const head = element("bp-box-head", root);
    const name = element("bp-box-name", head);
    const balls = element("bp-balls", head);
    const level = element("bp-box-level", head);
    const hp = element("bp-hp", root);
    element("bp-hp-label", hp).textContent = "HP";
    const track = element("bp-hp-track", hp);
    const hpFill = element("bp-hp-fill", track);

    // Only the viewer's own box has the numbers and the EXP bar, as in the games.
    if (seat === "far") {
      return { root, name, balls, level, hpFill, numbers: null, expFill: null };
    }
    const numbers = element("bp-hp-numbers", root);
    const exp = element("bp-exp", root);
    const expFill = element("bp-exp-fill", exp);
    return { root, name, balls, level, hpFill, numbers, expFill };
  }
}

/** The order the roulette visits `count` moves in. */
export function menuRound(count: number): readonly number[] {
  return MENU_ROUND.filter((index) => index < count);
}
