import { pokemonIdFromSeed } from "@/lib/avatar";
import { BUDDY_SPECIES } from "./species.generated";

declare const buddySpeciesIdBrand: unique symbol;

/** A dex number that is guaranteed to have a self-hosted buddy sprite atlas. */
export type BuddySpeciesId = number & { readonly [buddySpeciesIdBrand]: true };

/** A Pokemon the timer buddy can be. */
export type BuddySpecies = {
  readonly id: BuddySpeciesId;
  readonly name: string;
};

const SPECIES_NAMES: ReadonlyMap<number, string> = new Map(BUDDY_SPECIES);

/**
 * Refines an untrusted value, such as one read from localStorage, into a species id.
 * Returns `null` for anything that is not a dex number with a sprite atlas.
 */
export function parseBuddySpeciesId(value: unknown): BuddySpeciesId | null {
  if (typeof value !== "number" || !SPECIES_NAMES.has(value)) {
    return null;
  }

  // SAFETY: membership in SPECIES_NAMES is exactly the brand's invariant.
  return value as BuddySpeciesId;
}

/** Every available buddy, ordered by dex number. */
export const BUDDY_SPECIES_LIST: ReadonlyArray<BuddySpecies> =
  BUDDY_SPECIES.flatMap(([dex, name]) => {
    const id = parseBuddySpeciesId(dex);
    return id === null ? [] : [{ id, name }];
  });

/**
 * The buddy a user gets without choosing one: the Pokemon of their fallback avatar,
 * or the next dex number that has sprites when that one does not.
 */
export function defaultBuddySpeciesId(seed?: string | null): BuddySpeciesId {
  const avatarId = pokemonIdFromSeed(seed);
  const species =
    BUDDY_SPECIES_LIST.find((candidate) => candidate.id >= avatarId) ??
    BUDDY_SPECIES_LIST[0];
  if (!species) {
    throw new Error("No buddy sprites are bundled");
  }

  return species.id;
}

/** The stored choice when it is still a valid buddy, otherwise the seeded default. */
export function resolveBuddySpeciesId(
  selected: BuddySpeciesId | null,
  seed?: string | null,
): BuddySpeciesId {
  return selected ?? defaultBuddySpeciesId(seed);
}

/** A uniformly random buddy, for the shuffle button. */
export function randomBuddySpeciesId(): BuddySpeciesId {
  const species =
    BUDDY_SPECIES_LIST[Math.floor(Math.random() * BUDDY_SPECIES_LIST.length)];
  return species ? species.id : defaultBuddySpeciesId();
}

/** Display name of a buddy, such as "Pinsir". */
export function buddySpeciesName(id: BuddySpeciesId): string {
  return SPECIES_NAMES.get(id) ?? `#${id}`;
}
