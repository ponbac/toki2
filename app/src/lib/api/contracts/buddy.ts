import { z } from "zod";
import { type BuddySpeciesId, parseBuddySpeciesId } from "@/lib/buddy/species";

/** A calendar date in the server's buddy time zone, `YYYY-MM-DD`. */
const localDateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const timestampSchema = z.string().datetime({ offset: true });
const userIdSchema = z.number().int().positive();
const countSchema = z.number().int().nonnegative();
const levelSchema = z.number().int().min(1).max(100);

/** Every buddy the server knows has a sprite atlas, so ids parse into the sprite brand. */
const speciesIdSchema = z
  .number()
  .int()
  .transform((value, ctx): BuddySpeciesId => {
    const id = parseBuddySpeciesId(value);
    if (id === null) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `No buddy sprites for species ${value}`,
      });
      return z.NEVER;
    }
    return id;
  });

const buddyFields = {
  speciesId: speciesIdSchema,
  visible: z.boolean(),
  level: levelSchema,
  /** The slot's own XP. */
  xp: countSchema,
  /** Total XP at which the current level starts. */
  levelXp: countSchema,
  /** Total XP at which the next level starts, `null` at Lv 100. */
  nextLevelXp: z.number().int().positive().nullable(),
};

const leadBuddySchema = z
  .object({ slot: z.literal(1), ...buddyFields })
  .strict();
const partnerBuddySchema = z
  .object({ slot: z.literal(2), ...buddyFields })
  .strict();

const buddyTodaySchema = z
  .object({
    date: localDateSchema,
    loggedHours: z.number().nonnegative(),
    hoursXp: countSchema,
    battles: countSchema,
    battleXp: countSchema,
  })
  .strict();

const buddyProfileSchema = z
  .object({
    enabled: z.boolean(),
    battleNotifications: z.boolean(),
    // Slot order, so the lead always comes first.
    buddies: z.union([
      z.tuple([leadBuddySchema]),
      z.tuple([leadBuddySchema, partnerBuddySchema]),
    ]),
    partnerUnlockLevel: levelSchema,
    today: buddyTodaySchema,
    hoursSyncedAt: timestampSchema.nullable(),
  })
  .strict()
  .transform(({ buddies: [lead, partner], ...profile }) => ({
    ...profile,
    lead,
    partner: partner ?? null,
  }));

const buddyProfileResponseSchema = z
  .object({ profile: buddyProfileSchema.nullable() })
  .strict();

const updatedBuddyProfileResponseSchema = z
  .object({ profile: buddyProfileSchema })
  .strict();

const teamMemberSchema = z
  .object({ speciesId: speciesIdSchema, level: levelSchema })
  .strict();
/** A team that fights: the lead, then the partner when there is one. */
const teamSchema = z.array(teamMemberSchema).min(1).max(2);

const battleRecordSchema = z
  .object({ wins: countSchema, losses: countSchema, draws: countSchema })
  .strict();

const buddyOpponentSchema = z
  .object({
    userId: userIdSchema,
    fullName: z.string(),
    team: teamSchema,
    record: battleRecordSchema,
    battledToday: z.boolean(),
  })
  .strict();

const buddyOpponentsSchema = z
  .object({
    nextBattleAt: timestampSchema.nullable(),
    opponents: z.array(buddyOpponentSchema),
  })
  .strict();

const battleResultSchema = z.enum(["win", "loss", "draw"]);
const battleSideSchema = z.enum(["challenger", "defender"]);
const battleKindSchema = z.enum(["colleague", "trainer"]);

const buddyTrainerSchema = z
  .object({
    date: localDateSchema,
    name: z.string().min(1),
    team: teamSchema,
    battleId: z.number().int().positive().nullable(),
    result: battleResultSchema.nullable(),
  })
  .strict();

/** The Gen 5 types; there is no Fairy. */
const battleTypeSchema = z.enum([
  "Normal",
  "Fire",
  "Water",
  "Electric",
  "Grass",
  "Ice",
  "Fighting",
  "Poison",
  "Ground",
  "Flying",
  "Psychic",
  "Bug",
  "Rock",
  "Ghost",
  "Dragon",
  "Dark",
  "Steel",
]);

const battleMoveSchema = z
  .object({
    name: z.string().min(1),
    type: battleTypeSchema,
    category: z.enum(["physical", "special"]),
    /** Power as listed in the dex; the engine clamps it to 60–100. */
    power: z.number().int().positive(),
    /** `null` for moves that never miss. */
    accuracy: z.number().int().min(1).max(100).nullable(),
    pp: z.number().int().positive(),
  })
  .strict();

const battleMonSchema = z
  .object({
    speciesId: speciesIdSchema,
    name: z.string().min(1),
    level: levelSchema,
    types: z.array(battleTypeSchema).min(1).max(2),
    maxHp: z.number().int().positive(),
    moves: z.array(battleMoveSchema).min(1).max(4),
  })
  .strict();

const battleEventSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("send"),
      side: battleSideSchema,
      index: z.number().int().nonnegative(),
    })
    .strict(),
  z
    .object({
      type: z.literal("move"),
      side: battleSideSchema,
      moveIndex: z.number().int().nonnegative(),
    })
    .strict(),
  // The attacker.
  z.object({ type: z.literal("miss"), side: battleSideSchema }).strict(),
  // The target.
  z.object({ type: z.literal("immune"), side: battleSideSchema }).strict(),
  z
    .object({
      type: z.literal("hit"),
      // The target.
      side: battleSideSchema,
      damage: countSchema,
      hpAfter: countSchema,
      effectiveness: z.number().nonnegative(),
      crit: z.boolean(),
    })
    .strict(),
  z.object({ type: z.literal("faint"), side: battleSideSchema }).strict(),
  z
    .object({ type: z.literal("end"), winner: battleSideSchema.nullable() })
    .strict(),
]);

const battleOpponentSchema = z
  .object({ userId: userIdSchema.nullable(), name: z.string() })
  .strict();

const battleSummarySchema = z
  .object({
    id: z.number().int().positive(),
    kind: battleKindSchema,
    createdAt: timestampSchema,
    role: battleSideSchema,
    opponent: battleOpponentSchema,
    myTeam: teamSchema,
    theirTeam: teamSchema,
    result: battleResultSchema,
    /** Always true for the challenger. */
    seen: z.boolean(),
  })
  .strict();

const battleParticipantSchema = z
  .object({
    /** `null` for the daily trainer. */
    userId: userIdSchema.nullable(),
    name: z.string(),
    team: z.array(battleMonSchema).min(1).max(2),
  })
  .strict();

const battleLogSchema = battleSummarySchema
  .extend({
    seed: z.string().regex(/^[0-9a-f]{8}$/),
    engineVersion: z.number().int().positive(),
    challenger: battleParticipantSchema,
    defender: battleParticipantSchema,
    events: z.array(battleEventSchema).min(1),
  })
  .strict();

const battleDetailSchema = battleLogSchema.superRefine(checkBattleLog);

const battleListSchema = z
  .object({ battles: z.array(battleSummarySchema) })
  .strict();

const leaderboardRowSchema = z
  .object({
    userId: userIdSchema,
    fullName: z.string(),
    team: teamSchema,
    wins: countSchema,
    losses: countSchema,
    draws: countSchema,
    trainerWins: countSchema,
  })
  .strict();

const leaderboardSchema = z
  .object({
    month: z.string().regex(/^\d{4}-\d{2}$/),
    rows: z.array(leaderboardRowSchema),
  })
  .strict();

/**
 * Checks that every event refers to a buddy that is out on the field, so a
 * replay can index the teams without guarding each step.
 */
function checkBattleLog(
  battle: z.infer<typeof battleLogSchema>,
  ctx: z.RefinementCtx,
): void {
  const active: Record<BattleSide, BattleMon | null> = {
    challenger: null,
    defender: null,
  };
  const fail = (index: number, message: string) =>
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["events", index],
      message,
    });

  battle.events.forEach((event, index) => {
    switch (event.type) {
      case "send": {
        const mon = battle[event.side].team[event.index];
        if (mon) {
          active[event.side] = mon;
        } else {
          fail(index, `No ${event.side} buddy ${event.index}`);
        }
        return;
      }
      case "move": {
        const mon = active[event.side];
        if (!mon || event.moveIndex >= mon.moves.length) {
          fail(index, `No ${event.side} move ${event.moveIndex}`);
        }
        return;
      }
      case "hit": {
        const mon = active[event.side];
        if (!mon || event.hpAfter > mon.maxHp) {
          fail(index, `Hit on a ${event.side} buddy that is not out`);
        }
        return;
      }
      case "miss":
      case "immune":
      case "faint":
        if (!active[event.side]) {
          fail(index, `No ${event.side} buddy is out`);
        }
        return;
      case "end":
        if (index !== battle.events.length - 1) {
          fail(index, "The battle continues after its end");
        }
        return;
    }
  });

  if (battle.events.at(-1)?.type !== "end") {
    fail(battle.events.length - 1, "The battle has no end");
  }
}

/** One of the user's buddies with its level and XP. */
export type Buddy = Readonly<
  z.infer<typeof leadBuddySchema> | z.infer<typeof partnerBuddySchema>
>;

/** Which buddy slot: 1 is the lead, 2 the partner. */
export type BuddySlot = Buddy["slot"];

/** Today's logged hours, battles and the XP each gave, in the server's time zone. */
export type BuddyToday = Readonly<z.infer<typeof buddyTodaySchema>>;

/**
 * A user's buddy settings and team. The lead is always there once enrolled;
 * the partner is `null` until it is picked, which needs the lead at
 * `partnerUnlockLevel`. A hidden partner still fights.
 */
export type BuddyProfile = Readonly<z.infer<typeof buddyProfileSchema>>;

/**
 * Changes to the current user's buddy profile; omitted fields stay as they
 * are. The first update enrols the user and needs `enabled` and
 * `leadSpeciesId`. `partnerSpeciesId` creates the partner (the lead must have
 * reached the unlock level) or changes its species; `partnerVisible` needs a
 * partner.
 */
export type BuddyProfileUpdate = {
  readonly enabled?: boolean;
  readonly battleNotifications?: boolean;
  readonly leadSpeciesId?: BuddySpeciesId;
  readonly partnerSpeciesId?: BuddySpeciesId;
  readonly partnerVisible?: boolean;
};

/** One buddy of a team as others see it. */
export type BuddyTeamMember = Readonly<z.infer<typeof teamMemberSchema>>;

/** Head-to-head results against one colleague, from the current user's side. */
export type BattleRecord = Readonly<z.infer<typeof battleRecordSchema>>;

/** A colleague whose buddies can be battled, with the team that would fight. */
export type BuddyOpponent = Readonly<z.infer<typeof buddyOpponentSchema>>;

/**
 * Colleagues to battle, sorted by name. `nextBattleAt` is when the hourly
 * cooldown ends, or `null` when a battle can start now.
 */
export type BuddyOpponents = Readonly<z.infer<typeof buddyOpponentsSchema>>;

/** Today's shared trainer and how the current user's fight against it went. */
export type BuddyTrainer = Readonly<z.infer<typeof buddyTrainerSchema>>;

/** How a battle ended for the current user. */
export type BattleResult = z.infer<typeof battleResultSchema>;

/** The side that started a battle, or the side that was battled. */
export type BattleSide = z.infer<typeof battleSideSchema>;

/** A battle against a colleague's buddies or against the daily trainer. */
export type BattleKind = z.infer<typeof battleKindSchema>;

/** A Gen 5 type name. */
export type BattleType = z.infer<typeof battleTypeSchema>;

/** One of a battling buddy's moves. */
export type BattleMove = Readonly<z.infer<typeof battleMoveSchema>>;

/** A buddy as it was when the battle was fought. */
export type BattleMon = Readonly<z.infer<typeof battleMonSchema>>;

/**
 * One step of a battle log. `miss` names the attacker; `immune`, `hit` and
 * `faint` name the side that was targeted or fainted; `end` has a `null`
 * winner for a draw.
 */
export type BattleEvent = Readonly<z.infer<typeof battleEventSchema>>;

/** A battle in the current user's list, seen from their side. */
export type BattleSummary = Readonly<z.infer<typeof battleSummarySchema>>;

/** One side of a battle with its team snapshot. */
export type BattleParticipant = Readonly<
  z.infer<typeof battleParticipantSchema>
>;

/**
 * A battle with everything a replay needs. Every event refers to a buddy that
 * is out on its side, move indices exist, and the log ends with `end`.
 */
export type BattleDetail = Readonly<z.infer<typeof battleDetailSchema>>;

/** One user's started battles in a month. */
export type BuddyLeaderboardRow = Readonly<
  z.infer<typeof leaderboardRowSchema>
>;

/** The leaderboard of one local calendar month, `YYYY-MM`; rows are unsorted. */
export type BuddyLeaderboard = Readonly<z.infer<typeof leaderboardSchema>>;

/** Parses `GET /buddy`; `null` when the user has not enrolled yet. */
export function parseBuddyProfileResponse(input: unknown): BuddyProfile | null {
  return buddyProfileResponseSchema.parse(input).profile;
}

/** Parses the profile returned by `PUT /buddy`. */
export function parseUpdatedBuddyProfile(input: unknown): BuddyProfile {
  return updatedBuddyProfileResponseSchema.parse(input).profile;
}

/** Parses `GET /buddy/opponents`. */
export function parseBuddyOpponents(input: unknown): BuddyOpponents {
  return buddyOpponentsSchema.parse(input);
}

/** Parses `GET /buddy/trainer`. */
export function parseBuddyTrainer(input: unknown): BuddyTrainer {
  return buddyTrainerSchema.parse(input);
}

/** Parses `GET /buddy/battles`, newest first. */
export function parseBattleList(input: unknown): readonly BattleSummary[] {
  return battleListSchema.parse(input).battles;
}

/** Parses a battle with its teams and log, from a fetch or a new battle. */
export function parseBattleDetail(input: unknown): BattleDetail {
  return battleDetailSchema.parse(input);
}

/** Parses `GET /buddy/leaderboard`. */
export function parseBuddyLeaderboard(input: unknown): BuddyLeaderboard {
  return leaderboardSchema.parse(input);
}
