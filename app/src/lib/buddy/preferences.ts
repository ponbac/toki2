import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";

/** How long the buddies rest between idle behaviours. */
export type BuddyEnergy = "calm" | "lively";

/**
 * Read from localStorage on first use rather than after mount, so the first
 * render already has the stored energy.
 */
const READ_ON_INIT = { getOnInit: true };

const storedEnergyAtom = atomWithStorage<unknown>(
  "timer-buddy-energy",
  "calm",
  undefined,
  READ_ON_INIT,
);

/**
 * The buddies' energy level, a preference of this device rather than of the
 * account. Unknown stored values read as "calm".
 */
export const timerBuddyEnergyAtom = atom(
  (get): BuddyEnergy =>
    get(storedEnergyAtom) === "lively" ? "lively" : "calm",
  (_get, set, energy: BuddyEnergy) => {
    set(storedEnergyAtom, energy);
  },
);
