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

/** How fast battle replays play: normal or double speed. */
export type BuddyReplaySpeed = 1 | 2;

const storedReplaySpeedAtom = atomWithStorage<unknown>(
  "buddy-battle-replay-speed",
  1,
  undefined,
  READ_ON_INIT,
);

/**
 * The speed battle replays start at, a preference of this device. Unknown
 * stored values read as normal speed.
 */
export const buddyReplaySpeedAtom = atom(
  (get): BuddyReplaySpeed => (get(storedReplaySpeedAtom) === 2 ? 2 : 1),
  (_get, set, speed: BuddyReplaySpeed) => {
    set(storedReplaySpeedAtom, speed);
  },
);
