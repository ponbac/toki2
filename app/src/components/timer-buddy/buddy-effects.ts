import { formatBuddyMinutes } from "@/lib/buddy/format";

/** A feeling shown as a small bubble over the buddy's head. */
export type BuddyBubble = "alert" | "annoyed" | "sleep" | "heart" | "note";

/** Colour of floating value text: time banked or time thrown away. */
export type BuddyTextTone = "gain" | "loss";

/** Where an effect appears: the buddy's centre and the top of its head. */
export type BuddyAnchor = {
  readonly x: number;
  readonly headY: number;
};

/** A status plate whose EXP bar fills from one total toward the day's goal. */
export type BuddyPlate = {
  readonly name: string;
  /** Short label beside the name, such as "+1h24m". */
  readonly tag: string;
  readonly fromMinutes: number;
  readonly toMinutes: number;
  readonly goalMinutes: number;
};

type PlateState = BuddyPlate & {
  readonly root: HTMLElement;
  readonly box: HTMLElement;
  readonly fill: HTMLElement;
  readonly total: HTMLElement;
  elapsed: number;
  filled: boolean;
};

const TRANSIENT_ATTRIBUTE = "data-tb-transient";
const PLATE_WIDTH = 180;
/** Width of `.tb-bar-track` minus its padding, in CSS pixels. */
const BAR_FILL_WIDTH = 122;
const PLATE_FILL_DELAY_S = 0.35;
const PLATE_FILL_S = 0.9;
const PLATE_HOLD_S = 2.2;
const VIEWPORT_MARGIN = 8;

const HEART_PIXELS = [
  ".XX.XX.",
  "XXXXXXX",
  "XXXXXXX",
  ".XXXXX.",
  "..XXX..",
  "...X...",
];
const NOTE_PIXELS = [
  "..XXXX",
  "..X..X",
  "..X..X",
  "..X..X",
  ".XX.XX",
  "XXXXXX",
  ".X..X.",
];

const SVG_NAMESPACE = "http://www.w3.org/2000/svg";

function element(className: string, text?: string): HTMLElement {
  const node = document.createElement("div");
  node.className = className;
  if (text !== undefined) {
    node.textContent = text;
  }
  return node;
}

function pixelGlyph(rows: ReadonlyArray<string>): SVGSVGElement {
  const width = rows[0]?.length ?? 0;
  const svg = document.createElementNS(SVG_NAMESPACE, "svg");
  svg.setAttribute("width", String(width * 2));
  svg.setAttribute("height", String(rows.length * 2));
  svg.setAttribute("viewBox", `0 0 ${width} ${rows.length}`);
  svg.setAttribute("fill", "currentColor");
  svg.setAttribute("shape-rendering", "crispEdges");
  svg.style.display = "block";

  rows.forEach((row, y) => {
    [...row].forEach((pixel, x) => {
      if (pixel !== "X") {
        return;
      }
      const rect = document.createElementNS(SVG_NAMESPACE, "rect");
      rect.setAttribute("x", String(x));
      rect.setAttribute("y", String(y));
      rect.setAttribute("width", "1");
      rect.setAttribute("height", "1");
      svg.append(rect);
    });
  });

  return svg;
}

function bubbleContent(kind: BuddyBubble): Node {
  switch (kind) {
    case "alert":
      return element("tb-px16 tb-loss", "!");
    case "annoyed":
      return element("tb-px16 tb-loss", "!!");
    case "sleep":
      return element("tb-px8 tb-ink", "Zz");
    case "heart": {
      const wrap = element("tb-loss");
      wrap.append(pixelGlyph(HEART_PIXELS));
      return wrap;
    }
    case "note": {
      const wrap = element("tb-ink");
      wrap.append(pixelGlyph(NOTE_PIXELS));
      return wrap;
    }
  }
}

/**
 * Retro effect text drawn into the buddy layer: emote bubbles, floating values
 * and the status plate. Effects stay where they were spawned and remove themselves.
 */
export class BuddyEffects {
  private readonly layer: HTMLElement;
  private plateState: PlateState | null = null;

  constructor(layer: HTMLElement) {
    this.layer = layer;
    this.layer.addEventListener("animationend", this.removeFinished);
  }

  /** Pops a feeling bubble beside the buddy's head. */
  bubble(kind: BuddyBubble, anchor: BuddyAnchor): void {
    const bubble = element("tb-bubble tb-outline");
    const box = element("tb-box");
    box.append(bubbleContent(kind));
    const firstStep = element("tb-bubble-tail");
    firstStep.dataset.step = "1";
    const secondStep = element("tb-bubble-tail");
    secondStep.dataset.step = "2";
    bubble.append(box, firstStep, secondStep);

    this.spawn(bubble, anchor.x + 12, anchor.headY - 56, true);
  }

  /** Floats a short value, such as "+24m", above the buddy. */
  floatText(text: string, tone: BuddyTextTone, anchor: BuddyAnchor): void {
    const float = element(
      `tb-float tb-outline tb-px16 ${tone === "gain" ? "tb-gain" : "tb-loss"}`,
      text,
    );
    const root = this.spawn(float, anchor.x, anchor.headY - 58, true);
    root.style.translate = "-50% 0";
  }

  /** Shows the status plate, replacing one that is still on screen. */
  plate(plate: BuddyPlate, anchor: BuddyAnchor): void {
    this.plateState?.root.remove();

    const box = element("tb-box tb-frame tb-plate");
    const head = element("tb-px8 tb-plate-head");
    head.append(element("", plate.name), element("tb-gain", plate.tag));
    const bar = element("tb-bar");
    const track = element("tb-bar-track");
    const fill = element("tb-bar-fill");
    track.append(fill);
    bar.append(element("tb-px8 tb-bar-label", "EXP"), track);
    const total = element("tb-px8 tb-plate-total");
    box.append(head, bar, total);

    const outlined = element("tb-outline");
    outlined.append(box);
    const left = Math.min(
      Math.max(anchor.x - PLATE_WIDTH / 2, VIEWPORT_MARGIN),
      window.innerWidth - PLATE_WIDTH - VIEWPORT_MARGIN,
    );
    const root = this.spawn(
      outlined,
      left,
      Math.max(VIEWPORT_MARGIN, anchor.headY - 125),
      false,
    );

    this.plateState = { ...plate, root, box, fill, total, elapsed: 0, filled: false };
    this.drawPlate(this.plateState, 0);
  }

  /** Advances the plate's bar by `dt` seconds. */
  tick(dt: number): void {
    const plate = this.plateState;
    if (!plate) {
      return;
    }

    plate.elapsed += dt;
    const progress = Math.min(
      1,
      Math.max(0, (plate.elapsed - PLATE_FILL_DELAY_S) / PLATE_FILL_S),
    );
    if (!plate.filled) {
      this.drawPlate(plate, progress);
      plate.filled = progress >= 1;
    }
    if (plate.elapsed >= PLATE_FILL_DELAY_S + PLATE_FILL_S + PLATE_HOLD_S) {
      plate.root.remove();
      this.plateState = null;
    }
  }

  /** Removes every effect and stops listening. */
  destroy(): void {
    this.layer.removeEventListener("animationend", this.removeFinished);
    this.layer
      .querySelectorAll(".tb-effect")
      .forEach((effect) => effect.remove());
    this.plateState = null;
  }

  private drawPlate(plate: PlateState, progress: number): void {
    const minutes =
      plate.fromMinutes + (plate.toMinutes - plate.fromMinutes) * progress;
    const share = Math.min(1, minutes / plate.goalMinutes);
    // Two-pixel steps, so the bar grows like a sprite rather than a tween.
    plate.fill.style.width = `${Math.round((share * BAR_FILL_WIDTH) / 2) * 2}px`;

    const reachedGoal = progress >= 1 && plate.toMinutes >= plate.goalMinutes;
    plate.total.textContent = reachedGoal
      ? "GOAL!"
      : `${formatBuddyMinutes(minutes)}/${formatBuddyMinutes(plate.goalMinutes)}`;
    if (reachedGoal) {
      plate.box.dataset.full = "";
    }
  }

  private spawn(
    content: HTMLElement,
    left: number,
    top: number,
    transient: boolean,
  ): HTMLElement {
    const root = element("tb-effect");
    root.style.left = `${Math.round(left)}px`;
    root.style.top = `${Math.round(top)}px`;
    if (transient) {
      root.setAttribute(TRANSIENT_ATTRIBUTE, "");
    }
    root.append(content);
    this.layer.append(root);
    return root;
  }

  private readonly removeFinished = (event: AnimationEvent): void => {
    if (event.target instanceof Element) {
      event.target.closest(`[${TRANSIENT_ATTRIBUTE}]`)?.remove();
    }
  };
}
