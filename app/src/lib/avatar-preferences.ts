import { atomWithStorage } from "jotai/utils";

export const enablePokemonAvatarFallbackAtom = atomWithStorage(
  "avatar-enablePokemonFallback",
  false,
);
