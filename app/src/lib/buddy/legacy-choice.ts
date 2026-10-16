import { type BuddySpeciesId, parseBuddySpeciesId } from "./species";

// Where the buddy's on/off answer and Pokemon lived before they moved to the server.
const LEGACY_ENABLED_KEY = "timer-buddy-enabled";
const LEGACY_SPECIES_KEY = "timer-buddy-species";

/**
 * What this device remembers from before buddy profiles were stored on the
 * server. `enabled` is `null` when the user never answered the intro, and
 * `speciesId` is `null` when they followed their avatar's Pokemon.
 */
export type LegacyBuddyChoice = {
  readonly enabled: boolean | null;
  readonly speciesId: BuddySpeciesId | null;
};

/** Reads a jotai `atomWithStorage` value; storage that is blocked or holds junk reads as `null`. */
function readStoredJson(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? null : JSON.parse(raw);
  } catch {
    return null;
  }
}

/** Reads the device's pre-server buddy choice. Never throws. */
export function readLegacyBuddyChoice(): LegacyBuddyChoice {
  const enabled = readStoredJson(LEGACY_ENABLED_KEY);
  return {
    enabled: typeof enabled === "boolean" ? enabled : null,
    speciesId: parseBuddySpeciesId(readStoredJson(LEGACY_SPECIES_KEY)),
  };
}

/** Forgets the pre-server choice once the server has a profile. Never throws. */
export function forgetLegacyBuddyChoice(): void {
  try {
    window.localStorage.removeItem(LEGACY_ENABLED_KEY);
    window.localStorage.removeItem(LEGACY_SPECIES_KEY);
  } catch {
    // Blocked storage has nothing to forget.
  }
}
