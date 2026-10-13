import { queryOptions } from "@tanstack/react-query";
import { z } from "zod";
import type { BuddySpeciesId } from "./species";

/** Animations every buddy atlas contains. */
export const BUDDY_ANIMATIONS = [
  "Idle",
  "Walk",
  "Sleep",
  "Hop",
  "Hurt",
  "Rotate",
  "Charge",
  "Attack",
  "Swing",
  "Double",
] as const;

/** Name of an animation in a buddy atlas. */
export type BuddyAnimName = (typeof BUDDY_ANIMATIONS)[number];

const nonNegativeInt = z.number().int().nonnegative();

const animSchema = z.object({
  /** Frame width and height in sprite pixels. */
  w: z.number().int().positive(),
  h: z.number().int().positive(),
  /** Per-frame durations in 1/60 s ticks; also the frame count. */
  durations: z.array(z.number().positive()).min(1),
  /** First opaque row of the first frame, the top of the head. */
  top: nonNegativeInt,
  /** One past the last opaque row of the first frame, where the feet stand. */
  foot: nonNegativeInt,
  /** Atlas y offset of each facing direction this animation was packed with. */
  rows: z.object({
    D: nonNegativeInt.optional(),
    DR: nonNegativeInt.optional(),
    R: nonNegativeInt.optional(),
  }),
});

const sheetSchema = z.object({
  id: z.number().int().positive(),
  name: z.string().min(1),
  anims: z.object({
    Idle: animSchema,
    Walk: animSchema,
    Sleep: animSchema,
    Hop: animSchema,
    Hurt: animSchema,
    Rotate: animSchema,
    Charge: animSchema,
    Attack: animSchema,
    Swing: animSchema,
    Double: animSchema,
  }),
});

/** One animation's layout inside an atlas, in sprite pixels. */
export type BuddyAnim = z.infer<typeof animSchema>;

/** A parsed sprite manifest together with its decoded atlas image. */
export type BuddySheet = z.infer<typeof sheetSchema> & {
  readonly atlasUrl: string;
  /** Atlas dimensions in sprite pixels. */
  readonly atlasWidth: number;
  readonly atlasHeight: number;
};

function buddyAssetUrl(file: string): string {
  return `${import.meta.env.BASE_URL}buddy/${file}`;
}

/** Where the per-sprite credits required by the sprite licence are served. */
export const BUDDY_CREDITS_URL = buddyAssetUrl("CREDITS.md");

async function loadBuddySheet(
  id: BuddySpeciesId,
  signal: AbortSignal,
): Promise<BuddySheet> {
  const response = await fetch(buddyAssetUrl(`${id}.json`), { signal });
  if (!response.ok) {
    throw new Error(`Buddy manifest ${id} responded with ${response.status}`);
  }
  const manifest = sheetSchema.parse(await response.json());

  const atlas = new Image();
  atlas.src = buddyAssetUrl(`${id}.webp`);
  await atlas.decode();

  return {
    ...manifest,
    atlasUrl: atlas.src,
    atlasWidth: atlas.naturalWidth,
    atlasHeight: atlas.naturalHeight,
  };
}

/** Query definitions for the self-hosted buddy sprites. */
export const buddyQueries = {
  /** Loads and decodes one Pokemon's atlas. Rejects when either file is missing or malformed. */
  sheet: (id: BuddySpeciesId) =>
    queryOptions({
      queryKey: ["buddy", "sheet", id] as const,
      queryFn: ({ signal }) => loadBuddySheet(id, signal),
      staleTime: Infinity,
      retry: 1,
    }),
};
