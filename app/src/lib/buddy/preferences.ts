import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import { type BuddySpeciesId, parseBuddySpeciesId } from "./species";

/** How long the buddy rests between idle behaviours. */
export type BuddyEnergy = "calm" | "lively";

/** Whether the timer buddy is shown at all. */
export const timerBuddyEnabledAtom = atomWithStorage(
  "timer-buddy-enabled",
  true,
);

const storedSpeciesAtom = atomWithStorage<unknown>("timer-buddy-species", null);
const storedEnergyAtom = atomWithStorage<unknown>("timer-buddy-energy", "calm");

/**
 * The Pokemon the user picked, or `null` to follow their fallback avatar.
 * Stored values that no longer name an available buddy read as `null`.
 */
export const timerBuddySpeciesAtom = atom(
  (get) => parseBuddySpeciesId(get(storedSpeciesAtom)),
  (_get, set, speciesId: BuddySpeciesId | null) => {
    set(storedSpeciesAtom, speciesId);
  },
);

/** The buddy's energy level. Unknown stored values read as "calm". */
export const timerBuddyEnergyAtom = atom(
  (get): BuddyEnergy =>
    get(storedEnergyAtom) === "lively" ? "lively" : "calm",
  (_get, set, energy: BuddyEnergy) => {
    set(storedEnergyAtom, energy);
  },
);
