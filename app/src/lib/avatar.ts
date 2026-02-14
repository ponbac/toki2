export const POKEMON_MAX_ID = 649;
const POKEMON_SPRITE_BASE_URL =
  "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/other/official-artwork";
const FALLBACK_AVATAR_SEED = "unknown-user";

export function normalizeAvatarSeed(seed?: string | null): string {
  const normalized = seed?.trim().toLowerCase();
  return normalized && normalized.length > 0
    ? normalized
    : FALLBACK_AVATAR_SEED;
}

// Lightweight deterministic hash for stable avatar selection.
export function hashSeedToUint32(seed: string): number {
  let hash = 2166136261;

  for (let i = 0; i < seed.length; i += 1) {
    hash ^= seed.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }

  return hash >>> 0;
}

export function pokemonIdFromSeed(
  seed?: string | null,
  max = POKEMON_MAX_ID,
): number {
  const boundedMax = Number.isFinite(max) && max > 0 ? Math.floor(max) : POKEMON_MAX_ID;
  const normalizedSeed = normalizeAvatarSeed(seed);
  const hash = hashSeedToUint32(normalizedSeed);

  return (hash % boundedMax) + 1;
}

export function pokemonAvatarUrlFromSeed(seed?: string | null): string {
  const pokemonId = pokemonIdFromSeed(seed);
  return `${POKEMON_SPRITE_BASE_URL}/${pokemonId}.png`;
}

export function avatarHueFromSeed(seed?: string | null): number {
  const normalizedSeed = normalizeAvatarSeed(seed);
  const hash = hashSeedToUint32(normalizedSeed);
  return hash % 360;
}

export type AvatarThemeMode = "light" | "dark";

function hslToRgbNormalized(hue: number, saturation: number, lightness: number) {
  const h = ((hue % 360) + 360) % 360;
  const s = Math.min(Math.max(saturation, 0), 100) / 100;
  const l = Math.min(Math.max(lightness, 0), 100) / 100;

  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;

  let rPrime = 0;
  let gPrime = 0;
  let bPrime = 0;

  if (h < 60) {
    rPrime = c;
    gPrime = x;
  } else if (h < 120) {
    rPrime = x;
    gPrime = c;
  } else if (h < 180) {
    gPrime = c;
    bPrime = x;
  } else if (h < 240) {
    gPrime = x;
    bPrime = c;
  } else if (h < 300) {
    rPrime = x;
    bPrime = c;
  } else {
    rPrime = c;
    bPrime = x;
  }

  return {
    r: rPrime + m,
    g: gPrime + m,
    b: bPrime + m,
  };
}

function toLinearRgb(channel: number): number {
  return channel <= 0.03928
    ? channel / 12.92
    : ((channel + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(rgb: { r: number; g: number; b: number }): number {
  const r = toLinearRgb(rgb.r);
  const g = toLinearRgb(rgb.g);
  const b = toLinearRgb(rgb.b);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(foregroundLuminance: number, backgroundLuminance: number): number {
  const light = Math.max(foregroundLuminance, backgroundLuminance);
  const dark = Math.min(foregroundLuminance, backgroundLuminance);
  return (light + 0.05) / (dark + 0.05);
}

function avatarAccentHslFromSeed(
  seed: string | null | undefined,
  mode: AvatarThemeMode,
): {
  hue: number;
  saturation: number;
  lightness: number;
} {
  const hue = avatarHueFromSeed(seed);
  const saturation = mode === "light" ? 70 : 72;
  const targetBackground = mode === "light"
    ? hslToRgbNormalized(34, 7, 91)
    : hslToRgbNormalized(222, 18, 14);
  const targetBackgroundLuminance = relativeLuminance(targetBackground);
  const minimumContrast = 4.5;

  let lightness = mode === "light" ? 36 : 68;
  const direction = mode === "light" ? -1 : 1;

  for (let i = 0; i < 40; i += 1) {
    const rgb = hslToRgbNormalized(hue, saturation, lightness);
    const luminance = relativeLuminance(rgb);
    const contrast = contrastRatio(luminance, targetBackgroundLuminance);

    if (contrast >= minimumContrast) {
      break;
    }

    lightness = Math.min(Math.max(lightness + direction, 0), 100);
  }

  return { hue, saturation, lightness };
}

export function avatarRingColorFromSeed(
  seed: string | null | undefined,
  mode: AvatarThemeMode,
  alpha = 0.45,
): string {
  const boundedAlpha = Number.isFinite(alpha)
    ? Math.min(Math.max(alpha, 0), 1)
    : 0.45;
  const { hue, saturation, lightness } = avatarAccentHslFromSeed(seed, mode);

  return `hsla(${hue}, ${saturation}%, ${lightness}%, ${boundedAlpha})`;
}

export function avatarFallbackTextColorFromSeed(
  seed: string | null | undefined,
  mode: AvatarThemeMode,
): string {
  const { hue, saturation, lightness } = avatarAccentHslFromSeed(seed, mode);

  return `hsl(${hue} ${saturation}% ${lightness}%)`;
}

export function uniqueAvatarSources(
  sources: Array<string | null | undefined>,
): string[] {
  const unique = new Set<string>();

  for (const source of sources) {
    if (!source) {
      continue;
    }

    const trimmed = source.trim();
    if (!trimmed || unique.has(trimmed)) {
      continue;
    }

    unique.add(trimmed);
  }

  return Array.from(unique);
}

export function buildAvatarSources(options: {
  preferredSources: Array<string | null | undefined>;
  pokemonSeed?: string | null;
  enablePokemonFallback: boolean;
}): string[] {
  return uniqueAvatarSources([
    ...options.preferredSources,
    options.enablePokemonFallback
      ? pokemonAvatarUrlFromSeed(options.pokemonSeed)
      : null,
  ]);
}
