import type {
  BattleDetail,
  BattleMon,
  BattleParticipant,
  BattleSide,
  BattleType,
} from "@/lib/api/queries/buddy";
import { BATTLE_XP } from "@/lib/buddy/level";
import type { BuddySpeciesId } from "@/lib/buddy/species";
import { shortPersonName } from "../../-lib/format";

/*
 * Turns a stored battle into what the replay shows, one step per thing that
 * happens on screen, with the text box pages and the scene after each step.
 * Pure: the same battle always gives the same script, so a replay can start
 * over, skip to the end or be stepped through without running the battle
 * again.
 */

/**
 * A side of the screen. The viewer's buddies stand `near`, bottom left and
 * seen from behind; the other side stands `far`, top right.
 */
export type Seat = "near" | "far";

export const SEATS: readonly Seat[] = ["far", "near"];

/** Characters on one line of the text box; the pixel font is monospaced. */
export const LINE_CHARS = 30;

/** Lines the text box shows at once. */
const PAGE_LINES = 2;

/** A move as the move menu shows it. */
export type ScriptMove = {
  readonly name: string;
  readonly type: BattleType;
  readonly category: "physical" | "special";
  readonly pp: number;
};

/** A battling buddy, with its name in the capitals the games use. */
export type ScriptMon = {
  readonly speciesId: BuddySpeciesId;
  readonly name: string;
  readonly level: number;
  readonly maxHp: number;
  readonly moves: readonly ScriptMove[];
};

/** One seat's buddies at a moment of the battle. */
export type SeatScene = {
  /** The buddy out on the field; `null` before the first send-out and after it faints. */
  readonly active: number | null;
  /** HP of each team member. */
  readonly hp: readonly number[];
  readonly fainted: readonly boolean[];
  /** PP left of each team member's moves. */
  readonly ppLeft: readonly (readonly number[])[];
};

/** Everything on the battlefield at a moment of the battle. */
export type Scene = {
  readonly near: SeatScene;
  readonly far: SeatScene;
  /** The battle XP has been shown on the near buddy's EXP bar. */
  readonly expGained: boolean;
};

/** What a step does on screen, besides its text. */
export type Beat =
  | { readonly kind: "intro" }
  | { readonly kind: "send"; readonly seat: Seat; readonly index: number }
  | {
      readonly kind: "move";
      readonly seat: Seat;
      readonly moveIndex: number;
      /** The move hits, so it is animated; a miss or an immune target is only told. */
      readonly lands: boolean;
    }
  | { readonly kind: "miss"; readonly seat: Seat }
  | { readonly kind: "immune"; readonly seat: Seat }
  | {
      readonly kind: "hit";
      /** The buddy that was hit. */
      readonly seat: Seat;
      readonly hpBefore: number;
      readonly hpAfter: number;
      readonly effectiveness: number;
      readonly crit: boolean;
    }
  /** Only text: a critical hit, the type effectiveness. */
  | { readonly kind: "note" }
  | { readonly kind: "faint"; readonly seat: Seat }
  | { readonly kind: "end"; readonly winner: Seat | null }
  /** The battle XP fills the near buddy's EXP bar; only for the challenger. */
  | { readonly kind: "exp" };

export type Step = {
  readonly beat: Beat;
  /** Text box pages shown during the step, each at most two lines. */
  readonly pages: readonly string[];
  /** The scene once the step is over. */
  readonly scene: Scene;
};

/** One page of text, as the replay shows it when stepped through. */
export type Stop = {
  readonly text: string;
  /** The scene shown with the page: everything up to the next page. */
  readonly scene: Scene;
};

export type BattleScript = {
  readonly teams: Readonly<Record<Seat, readonly ScriptMon[]>>;
  /** Before anything happens: nobody is out yet. */
  readonly initial: Scene;
  readonly steps: readonly Step[];
  readonly stops: readonly Stop[];
  /** The text of the last page. */
  readonly finalText: string;
  readonly finalScene: Scene;
};

/** The parts of a battle a replay depends on. */
export type BattleScriptInput = Pick<
  BattleDetail,
  "role" | "kind" | "challenger" | "defender" | "events"
>;

/**
 * The script of a battle as its viewer sees it. The viewer is the battle's
 * `role`, which the server sets for the user who asked for it.
 */
export function buildBattleScript(battle: BattleScriptInput): BattleScript {
  const nearSide = battle.role;
  const seatOf = (side: BattleSide): Seat =>
    side === nearSide ? "near" : "far";
  const farParticipant =
    nearSide === "challenger" ? battle.defender : battle.challenger;
  const againstTrainer = battle.kind === "trainer";
  const farName = participantName(farParticipant, againstTrainer);
  const teams: Record<Seat, readonly ScriptMon[]> = {
    near: battle[nearSide].team.map(scriptMon),
    far: farParticipant.team.map(scriptMon),
  };

  const state = {
    near: initialSeat(teams.near),
    far: initialSeat(teams.far),
    expGained: false,
  };
  const initial = snapshot(state);
  // The buddy each seat had out last, for the end of the battle.
  const lastOut: Record<Seat, number> = { near: 0, far: 0 };

  const steps: Step[] = [];
  const add = (beat: Beat, ...messages: string[]) =>
    steps.push({
      beat,
      pages: messages.flatMap(paginate),
      scene: snapshot(state),
    });
  const activeMon = (seat: Seat): ScriptMon | null => {
    const index = state[seat].active;
    return index === null ? null : (teams[seat][index] ?? null);
  };
  // "Foe" tells the far buddy apart, as the games do, even in a mirror match.
  const who = (seat: Seat): string => {
    const name = activeMon(seat)?.name ?? "???";
    return seat === "far" ? `Foe ${name}` : name;
  };

  add(
    { kind: "intro" },
    nearSide === "challenger"
      ? `${farName} wants to battle!`
      : `${farName} battled your buddies!`,
  );

  battle.events.forEach((event, eventIndex) => {
    switch (event.type) {
      case "send": {
        const seat = seatOf(event.side);
        state[seat] = { ...state[seat], active: event.index };
        lastOut[seat] = event.index;
        const name = activeMon(seat)?.name ?? "???";
        add(
          { kind: "send", seat, index: event.index },
          seat === "far" ? `${farName} sent out ${name}!` : `Go! ${name}!`,
        );
        return;
      }
      case "move": {
        const seat = seatOf(event.side);
        const mon = activeMon(seat);
        const move = mon?.moves[event.moveIndex];
        state[seat] = spendPp(state[seat], event.moveIndex);
        add(
          {
            kind: "move",
            seat,
            moveIndex: event.moveIndex,
            lands: battle.events[eventIndex + 1]?.type === "hit",
          },
          `${who(seat)} used ${move?.name ?? "???"}!`,
        );
        return;
      }
      case "miss": {
        const seat = seatOf(event.side);
        add({ kind: "miss", seat }, `${who(seat)}'s attack missed!`);
        return;
      }
      case "immune": {
        const seat = seatOf(event.side);
        add({ kind: "immune", seat }, `It doesn't affect ${who(seat)}...`);
        return;
      }
      case "hit": {
        const seat = seatOf(event.side);
        const index = state[seat].active;
        const hpBefore = index === null ? 0 : (state[seat].hp[index] ?? 0);
        if (index !== null) {
          state[seat] = {
            ...state[seat],
            hp: state[seat].hp.map((hp, member) =>
              member === index ? event.hpAfter : hp,
            ),
          };
        }
        add({
          kind: "hit",
          seat,
          hpBefore,
          hpAfter: event.hpAfter,
          effectiveness: event.effectiveness,
          crit: event.crit,
        });
        if (event.crit) {
          add({ kind: "note" }, "A critical hit!");
        }
        if (event.effectiveness > 1) {
          add({ kind: "note" }, "It's super effective!");
        } else if (event.effectiveness < 1) {
          add({ kind: "note" }, "It's not very effective...");
        }
        return;
      }
      case "faint": {
        const seat = seatOf(event.side);
        const message = `${who(seat)} fainted!`;
        const index = state[seat].active;
        state[seat] = {
          ...state[seat],
          active: null,
          fainted: state[seat].fainted.map(
            (fainted, member) => fainted || member === index,
          ),
        };
        add({ kind: "faint", seat }, message);
        return;
      }
      case "end": {
        const winner = event.winner === null ? null : seatOf(event.winner);
        add(
          { kind: "end", winner },
          ...endMessages({
            winner,
            farName,
            farLastMon: teams.far[lastOut.far]?.name ?? "buddy",
            nearStarted: nearSide === "challenger",
            againstTrainer,
          }),
        );
        // Defending gives no XP, so a defender is never told about any.
        if (nearSide === "challenger") {
          state.expGained = true;
          add({ kind: "exp" }, `Your buddies gained ${BATTLE_XP} EXP. Points!`);
        }
        return;
      }
    }
  });

  const stops = stepStops(steps);
  const finalScene = steps.at(-1)?.scene ?? initial;
  return {
    teams,
    initial,
    steps,
    stops,
    finalText: stops.at(-1)?.text ?? "",
    finalScene,
  };
}

/** The far trainer's name: a colleague's first name and initial, or the daily trainer. */
function participantName(
  participant: BattleParticipant,
  trainer: boolean,
): string {
  const name = trainer
    ? participant.name.trim()
    : shortPersonName(participant.name);
  return (name || "Someone").toUpperCase();
}

function scriptMon(mon: BattleMon): ScriptMon {
  return {
    speciesId: mon.speciesId,
    name: mon.name.toUpperCase(),
    level: mon.level,
    maxHp: mon.maxHp,
    moves: mon.moves.map((move) => ({
      name: move.name.toUpperCase(),
      type: move.type,
      category: move.category,
      pp: move.pp,
    })),
  };
}

function initialSeat(team: readonly ScriptMon[]): SeatScene {
  return {
    active: null,
    hp: team.map((mon) => mon.maxHp),
    fainted: team.map(() => false),
    ppLeft: team.map((mon) => mon.moves.map((move) => move.pp)),
  };
}

function spendPp(seat: SeatScene, moveIndex: number): SeatScene {
  const index = seat.active;
  if (index === null) {
    return seat;
  }
  return {
    ...seat,
    ppLeft: seat.ppLeft.map((moves, member) =>
      member === index
        ? moves.map((pp, move) =>
            move === moveIndex ? Math.max(0, pp - 1) : pp,
          )
        : moves,
    ),
  };
}

/**
 * The working state as a scene. Seats are replaced rather than changed in
 * place, so later steps never change an earlier scene.
 */
function snapshot(state: {
  near: SeatScene;
  far: SeatScene;
  expGained: boolean;
}): Scene {
  return { near: state.near, far: state.far, expGained: state.expGained };
}

function endMessages({
  winner,
  farName,
  farLastMon,
  nearStarted,
  againstTrainer,
}: {
  winner: Seat | null;
  farName: string;
  farLastMon: string;
  nearStarted: boolean;
  againstTrainer: boolean;
}): string[] {
  if (winner === "near") {
    return [
      `You defeated ${farName}!`,
      againstTrainer
        ? `${farName}: "Your buddies log a lot of hours!"`
        : `${farName}: "My ${farLastMon} was on a coffee break..."`,
    ];
  }
  if (winner === "far") {
    const rematch = !nearStarted
      ? ""
      : againstTrainer
        ? " Try again tomorrow!"
        : " Rematch tomorrow!";
    return [
      "You have no buddies left to fight!",
      `${farName} wins this one!${rematch}`,
    ];
  }
  return ["Both sides got tired. It's a draw!"];
}

/**
 * Splits a message into text box pages: words wrapped at `LINE_CHARS`, two
 * lines a page. Wrapping up front keeps a word from jumping to the next line
 * while it is being typed out.
 */
export function paginate(message: string): string[] {
  const lines: string[] = [];
  let line = "";
  for (const word of message.split(/\s+/).filter(Boolean)) {
    const candidate = line ? `${line} ${word}` : word;
    if (candidate.length <= LINE_CHARS) {
      line = candidate;
      continue;
    }
    if (line) {
      lines.push(line);
    }
    // A word longer than a line is cut, which only a very long name could need.
    let rest = word;
    while (rest.length > LINE_CHARS) {
      lines.push(rest.slice(0, LINE_CHARS));
      rest = rest.slice(LINE_CHARS);
    }
    line = rest;
  }
  if (line) {
    lines.push(line);
  }

  const pages: string[] = [];
  for (let start = 0; start < lines.length; start += PAGE_LINES) {
    pages.push(lines.slice(start, start + PAGE_LINES).join("\n"));
  }
  return pages;
}

/**
 * Every page of text with the scene to show beside it. A page shows what
 * happens up to the next page, so a move's text comes with the damage it
 * dealt, and the last page comes with the final scene.
 */
function stepStops(steps: readonly Step[]): Stop[] {
  const pages = steps.flatMap((step, stepIndex) =>
    step.pages.map((text) => ({ text, stepIndex })),
  );
  return pages.map(({ text, stepIndex }, pageIndex) => {
    const next = pages[pageIndex + 1];
    const sceneIndex =
      next === undefined
        ? steps.length - 1
        : Math.max(stepIndex, next.stepIndex - 1);
    const scene = steps[sceneIndex]?.scene;
    if (!scene) {
      throw new Error(`No step ${sceneIndex} in a battle script`);
    }
    return { text, scene };
  });
}
