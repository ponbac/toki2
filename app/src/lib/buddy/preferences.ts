import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import { type BuddySpeciesId, parseBuddySpeciesId } from "./species";

/** How long the buddy rests between idle behaviours. */
export type BuddyEnergy = "calm" | "lively";

/**
 * Read from localStorage on first use rather than after mount. Otherwise the
 * first render sees the defaults: a switched-off buddy would still load and
 * flash, and a picked Pokemon would briefly be the default one.
 */
const READ_ON_INIT = { getOnInit: true };

const storedEnabledAtom = atomWithStorage<unknown>(
  "timer-buddy-enabled",
  null,
  undefined,
  READ_ON_INIT,
);
const storedSpeciesAtom = atomWithStorage<unknown>(
  "timer-buddy-species",
  null,
  undefined,
  READ_ON_INIT,
);
const storedEnergyAtom = atomWithStorage<unknown>(
  "timer-buddy-energy",
  "calm",
  undefined,
  READ_ON_INIT,
);

/** Whether the timer buddy is shown. It stays off until the user turns it on. */
export const timerBuddyEnabledAtom = atom(
  (get) => get(storedEnabledAtom) === true,
  (_get, set, enabled: boolean) => {
    set(storedEnabledAtom, enabled);
  },
);

/**
 * Whether the user has said yes or no to the buddy, in the intro toast or in
 * the settings. Until then the intro toast offers it.
 */
export const timerBuddyAnsweredAtom = atom(
  (get) => typeof get(storedEnabledAtom) === "boolean",
);

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
