/**
 * Build the timer buddy battle dex, `toki-api/data/buddy_dex.json`, from the
 * generation 5 data in @pkmn/dex. The backend embeds the file, so the battle engine
 * needs no game data at runtime.
 *
 * The species are exactly the buddies that have sprites
 * (`src/lib/buddy/species.generated.ts`), so rerun this whenever the sprites are
 * regenerated. From `app/`:
 *
 *     bun run buddy:dex
 *
 * Every species gets up to four pre-picked moves: its strongest level-up moves at or
 * below Lv 50, preferring its own types and then type coverage, topped up with simple
 * fallback moves when it learns too few. The output is deterministic.
 */

import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Dex } from "@pkmn/dex";
import {
  Generations,
  type Generation,
  type Move,
  type Specie,
} from "@pkmn/data";
import dexPackage from "@pkmn/dex/package.json";
import { BUDDY_SPECIES } from "../src/lib/buddy/species.generated";

const OUTPUT_PATH = fileURLToPath(
  new URL("../../toki-api/data/buddy_dex.json", import.meta.url),
);
const GENERATION = 5;
const MAX_DEX_NUMBER = 649;
const MAX_MOVE_LEVEL = 50;
const MOVE_SLOTS = 4;
const LEVEL_UP_SOURCE = /^5L(\d+)$/;

/** Damaging moves that need rules the engine does not have. */
const EXCLUDED_MOVE_IDS: ReadonlySet<string> = new Set([
  "dreameater",
  "focuspunch",
  "fakeout",
  "suckerpunch",
  "lastresort",
  "synchronoise",
  "snore",
  "futuresight",
  "doomdesire",
  "skydrop",
  "feint",
  "naturalgift",
  "fling",
  "hiddenpower",
  "struggle",
]);

/**
 * Names of the fallback moves per type, as [physical 60, physical 70, special 60,
 * special 70]. The key order is the order of `types` in the output.
 */
const FALLBACK_MOVES = {
  Normal: ["Headbutt", "Slam", "Swift", "Hyper Voice"],
  Fire: ["Fire Fang", "Fire Punch", "Ember", "Flamethrower"],
  Water: ["Aqua Tail", "Waterfall", "Water Pulse", "Surf"],
  Electric: ["Thunder Fang", "Thunder Punch", "Shock Wave", "Thunderbolt"],
  Grass: ["Seed Bomb", "Leaf Blade", "Magical Leaf", "Energy Ball"],
  Ice: ["Ice Fang", "Ice Punch", "Icy Wind", "Ice Beam"],
  Fighting: ["Brick Break", "Sky Uppercut", "Aura Sphere", "Focus Blast"],
  Poison: ["Poison Jab", "Cross Poison", "Sludge", "Sludge Bomb"],
  Ground: ["Bulldoze", "Earthquake", "Mud Shot", "Earth Power"],
  Flying: ["Wing Attack", "Aerial Ace", "Air Cutter", "Air Slash"],
  Psychic: ["Zen Headbutt", "Psycho Cut", "Psybeam", "Psychic"],
  Bug: ["Bug Bite", "X-Scissor", "Signal Beam", "Bug Buzz"],
  Rock: ["Rock Tomb", "Rock Slide", "Ancient Power", "Power Gem"],
  Ghost: ["Shadow Claw", "Shadow Punch", "Ominous Wind", "Shadow Ball"],
  Dragon: ["Dragon Claw", "Dragon Tail", "Dragon Breath", "Dragon Pulse"],
  Dark: ["Bite", "Crunch", "Snarl", "Dark Pulse"],
  Steel: ["Metal Claw", "Iron Head", "Mirror Shot", "Flash Cannon"],
} as const satisfies Record<string, readonly [string, string, string, string]>;

/** The 17 generation 5 types, in the conventional order. */
type BattleType = keyof typeof FALLBACK_MOVES;

const BATTLE_TYPES: ReadonlyArray<BattleType> =
  Object.keys(FALLBACK_MOVES).filter(isBattleType);

type MoveCategory = "physical" | "special";

/** A move as the backend reads it. `power` already includes the expected hit count. */
type DexMove = {
  readonly name: string;
  readonly type: BattleType;
  readonly category: MoveCategory;
  readonly power: number;
  readonly accuracy: number | null;
  readonly priority: number;
  readonly critStage: number;
  readonly pp: number;
};

type DexSpecies = {
  readonly name: string;
  readonly types: ReadonlyArray<BattleType>;
  readonly baseStats: readonly [number, number, number, number, number, number];
  readonly moves: ReadonlyArray<DexMove>;
};

/** A learnable move with the values the picker ranks by. */
type Candidate = {
  readonly move: DexMove;
  readonly num: number;
  readonly score: number;
};

type PickedMoves = {
  readonly moves: ReadonlyArray<DexMove>;
  readonly usedFallbacks: boolean;
};

function isBattleType(name: string): name is BattleType {
  return Object.hasOwn(FALLBACK_MOVES, name);
}

function battleType(name: string, context: string): BattleType {
  if (!isBattleType(name)) {
    throw new Error(
      `${context} has type ${name}, which is not a generation 5 type`,
    );
  }
  return name;
}

/** Average hits per use: 2-5 hit moves follow the generation 5 odds (3.17). */
function expectedHits(move: Move): number {
  const { multihit } = move;
  if (multihit === undefined) {
    return 1;
  }
  if (typeof multihit === "number") {
    return multihit;
  }
  const [min, max] = multihit;
  if (min === undefined || max === undefined || multihit.length !== 2) {
    throw new Error(
      `${move.name} has an unexpected hit range ${JSON.stringify(multihit)}`,
    );
  }
  return min === 2 && max === 5 ? 3.17 : (min + max) / 2;
}

/** Whether the engine can use the move as a plain damaging attack. */
function isUsableMove(move: Move): boolean {
  return (
    move.category !== "Status" &&
    move.basePower > 0 &&
    !move.flags.recharge &&
    !move.flags.charge &&
    !move.selfdestruct &&
    !move.ohko &&
    !move.damage &&
    !move.willCrit &&
    !EXCLUDED_MOVE_IDS.has(move.id)
  );
}

function toCandidate(move: Move): Candidate {
  if (move.category === "Status") {
    throw new Error(`${move.name} is a status move`);
  }
  const power = Math.round(move.basePower * expectedHits(move));
  const accuracy = move.accuracy === true ? null : move.accuracy;
  return {
    move: {
      name: move.name,
      type: battleType(move.type, move.name),
      category: move.category === "Physical" ? "physical" : "special",
      power,
      accuracy,
      priority: move.priority,
      critStage: Math.max(0, (move.critRatio ?? 1) - 1),
      pp: move.pp,
    },
    num: move.num,
    score: (power * (accuracy ?? 100)) / 100,
  };
}

function fallbackMove(
  type: BattleType,
  power: 60 | 70,
  category: MoveCategory,
): DexMove {
  const [physical60, physical70, special60, special70] = FALLBACK_MOVES[type];
  const name =
    category === "physical"
      ? power === 60
        ? physical60
        : physical70
      : power === 60
        ? special60
        : special70;
  return {
    name,
    type,
    category,
    power,
    accuracy: 100,
    priority: 0,
    critStage: 0,
    pp: 20,
  };
}

/** The Normal 50-power move that tops up a species with too few moves. */
function fillerMove(
  category: MoveCategory,
  picked: ReadonlyArray<DexMove>,
): DexMove {
  const name =
    category === "physical"
      ? "Tackle"
      : picked.some((move) => move.name === "Swift")
        ? "Hyper Voice"
        : "Swift";
  return {
    name,
    type: "Normal",
    category,
    power: 50,
    accuracy: 100,
    priority: 0,
    critStage: 0,
    pp: 20,
  };
}

/**
 * The species' moves: the best move of each own type, then the best moves of types
 * not yet covered, then the best of the rest. Species that learn fewer than four get
 * fallbacks: a 60-power move of each own type it has no move for, a Normal 50-power
 * filler, then a 70-power move per own type, so a species can end up with three.
 */
function pickMoves(
  types: ReadonlyArray<BattleType>,
  category: MoveCategory,
  candidates: ReadonlyArray<Candidate>,
): PickedMoves {
  const ranked = [...candidates].sort(
    (a, b) => b.score - a.score || a.num - b.num,
  );
  const picked: Candidate[] = [];

  for (const type of types) {
    const best = ranked.find(
      (candidate) =>
        candidate.move.type === type && !picked.includes(candidate),
    );
    if (best) {
      picked.push(best);
    }
  }
  for (const candidate of ranked) {
    if (picked.length >= MOVE_SLOTS) {
      break;
    }
    const coversNewType = !picked.some(
      (other) => other.move.type === candidate.move.type,
    );
    if (!picked.includes(candidate) && coversNewType) {
      picked.push(candidate);
    }
  }
  for (const candidate of ranked) {
    if (picked.length >= MOVE_SLOTS) {
      break;
    }
    if (!picked.includes(candidate)) {
      picked.push(candidate);
    }
  }

  const moves: DexMove[] = picked.map((candidate) => candidate.move);
  const learnedCount = moves.length;
  for (const type of types) {
    if (
      moves.length < MOVE_SLOTS &&
      !moves.some((move) => move.type === type)
    ) {
      moves.push(fallbackMove(type, 60, category));
    }
  }
  if (moves.length < MOVE_SLOTS) {
    const filler = fillerMove(category, moves);
    if (!moves.some((move) => move.name === filler.name)) {
      moves.push(filler);
    }
  }
  for (const type of types) {
    if (moves.length < MOVE_SLOTS) {
      moves.push(fallbackMove(type, 70, category));
    }
  }

  return { moves, usedFallbacks: moves.length > learnedCount };
}

/** Damaging generation 5 level-up moves the species learns at or below Lv 50. */
async function levelUpCandidates(
  gen: Generation,
  specie: Specie,
): Promise<Candidate[]> {
  const learnset = (await gen.learnsets.get(specie.id))?.learnset;
  if (!learnset) {
    throw new Error(`${specie.name} has no learnset`);
  }

  const candidates: Candidate[] = [];
  for (const [moveId, sources] of Object.entries(learnset)) {
    const learnedInTime = sources.some((source) => {
      const level = LEVEL_UP_SOURCE.exec(source)?.[1];
      return level !== undefined && Number(level) <= MAX_MOVE_LEVEL;
    });
    if (!learnedInTime) {
      continue;
    }
    const move = gen.moves.get(moveId);
    if (!move) {
      throw new Error(
        `${specie.name} learns ${moveId}, which is missing from generation ${GENERATION}`,
      );
    }
    if (isUsableMove(move)) {
      candidates.push(toCandidate(move));
    }
  }
  return candidates;
}

/** Multipliers by attacking type, keeping only the entries that are not 1. */
type TypeChart = ReadonlyArray<
  readonly [BattleType, Readonly<Record<string, number>>]
>;

function typeChart(gen: Generation): TypeChart {
  const genTypes = [...gen.types].map((type) => type.name).sort();
  if (genTypes.join() !== [...BATTLE_TYPES].sort().join()) {
    throw new Error(
      `Generation ${GENERATION} types changed: ${genTypes.join(", ")}`,
    );
  }

  return BATTLE_TYPES.map((attacking) => {
    const row: Record<string, number> = {};
    for (const defending of BATTLE_TYPES) {
      const multiplier = gen.types.totalEffectiveness(attacking, [defending]);
      if (![0, 0.5, 1, 2].includes(multiplier)) {
        throw new Error(
          `${attacking} against ${defending} has multiplier ${multiplier}`,
        );
      }
      if (multiplier !== 1) {
        row[defending] = multiplier;
      }
    }
    return [attacking, row] as const;
  });
}

/**
 * Checks that every fallback name is a real generation 5 move of its type, so a
 * replay never shows a made-up move.
 */
function checkFallbackNames(gen: Generation): void {
  const names: Array<readonly [BattleType, string]> = BATTLE_TYPES.flatMap(
    (type) => FALLBACK_MOVES[type].map((name) => [type, name] as const),
  );
  names.push(["Normal", "Tackle"]);
  for (const [type, name] of names) {
    const move = gen.moves.get(name);
    if (!move || move.type !== type) {
      throw new Error(
        `Fallback ${name} is not a generation ${GENERATION} ${type} move`,
      );
    }
  }
}

function render(
  source: string,
  chart: TypeChart,
  species: ReadonlyArray<readonly [number, DexSpecies]>,
): string {
  const chartLines = chart.map(
    ([type, row]) => `    ${JSON.stringify(type)}: ${JSON.stringify(row)}`,
  );
  const speciesLines = species.map(
    ([num, entry]) => `    "${num}": ${JSON.stringify(entry)}`,
  );
  return [
    "{",
    `  "source": ${JSON.stringify(source)},`,
    `  "types": ${JSON.stringify(BATTLE_TYPES)},`,
    `  "typeChart": {`,
    chartLines.join(",\n"),
    "  },",
    `  "species": {`,
    speciesLines.join(",\n"),
    "  }",
    "}",
    "",
  ].join("\n");
}

async function main(): Promise<void> {
  const gen = new Generations(Dex).get(GENERATION);
  checkFallbackNames(gen);
  const chart = typeChart(gen);

  const speciesByNum = new Map<number, Specie>();
  for (const specie of gen.species) {
    if (specie.num < 1 || specie.num > MAX_DEX_NUMBER || specie.forme) {
      continue;
    }
    if (speciesByNum.has(specie.num)) {
      throw new Error(`Two base species share dex number ${specie.num}`);
    }
    speciesByNum.set(specie.num, specie);
  }

  const species: Array<readonly [number, DexSpecies]> = [];
  const short: string[] = [];
  const withFallbacks: string[] = [];
  const buddies = [...BUDDY_SPECIES].sort(([a], [b]) => a - b);
  for (const [num, name] of buddies) {
    const specie = speciesByNum.get(num);
    if (!specie) {
      throw new Error(
        `Buddy #${num} ${name} is missing from generation ${GENERATION}`,
      );
    }
    const types = specie.types.map((type) => battleType(type, specie.name));
    const { hp, atk, def, spa, spd, spe } = specie.baseStats;
    const category: MoveCategory = atk >= spa ? "physical" : "special";
    const { moves, usedFallbacks } = pickMoves(
      types,
      category,
      await levelUpCandidates(gen, specie),
    );
    const moveNames = new Set(moves.map((move) => move.name));
    if (moveNames.size !== moves.length) {
      throw new Error(
        `${name} would show two moves with the same name: ${moves.map((move) => move.name).join(", ")}`,
      );
    }

    species.push([
      num,
      { name, types, baseStats: [hp, atk, def, spa, spd, spe], moves },
    ]);
    if (moves.length < MOVE_SLOTS) {
      short.push(`${name} (${moves.length})`);
    }
    if (usedFallbacks) {
      withFallbacks.push(name);
    }
  }

  const source = `@pkmn/dex ${dexPackage.version}, generation ${GENERATION}`;
  await writeFile(OUTPUT_PATH, render(source, chart, species));

  console.log(
    `Wrote ${species.length} species from ${source} to ${OUTPUT_PATH}`,
  );
  console.log(
    `Fewer than ${MOVE_SLOTS} moves (${short.length}): ${short.join(", ") || "none"}`,
  );
  console.log(
    `Using fallback moves (${withFallbacks.length}): ${withFallbacks.join(", ") || "none"}`,
  );
}

await main();
