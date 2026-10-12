/**
 * Things the floating timer tells the buddy about. Positions are viewport x
 * coordinates in CSS pixels; durations are the timer's elapsed seconds.
 */
export type BuddyEvent =
  | { readonly type: "saveHover"; readonly x: number | null }
  | { readonly type: "deleteHover"; readonly active: boolean }
  | { readonly type: "noteFocus" }
  | { readonly type: "noteBlur"; readonly changed: boolean }
  | { readonly type: "saveStarted" }
  | {
      readonly type: "saved";
      readonly seconds: number;
      /** False when the entry belongs to an earlier day, such as a timer left running overnight. */
      readonly countsTowardToday: boolean;
    }
  | { readonly type: "deleted"; readonly seconds: number };

type BuddyEventListener = (event: BuddyEvent) => void;

const listeners = new Set<BuddyEventListener>();

/** Notifies the buddy, if one is mounted. A no-op when the buddy is switched off. */
export function emitBuddyEvent(event: BuddyEvent): void {
  for (const listener of listeners) {
    listener(event);
  }
}

/** Registers a listener and returns the function that removes it again. */
export function subscribeBuddyEvents(listener: BuddyEventListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
