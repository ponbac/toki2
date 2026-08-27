import { describe, expect, test } from "bun:test";
import {
  confirmTimerNoteDraftSaved,
  editTimerNoteDraft,
  isTimerNoteDraftDirty,
  syncTimerNoteDraft,
  type TimerNoteDraft,
} from "../src/lib/timer-note-draft";

const cleanDraft = (overrides: Partial<TimerNoteDraft> = {}) => ({
  timerStartTime: "timer-1",
  note: "server note",
  serverNote: "server note",
  ...overrides,
});

describe("timer note draft", () => {
  test("preserves a dirty draft when polling the same timer", () => {
    const draft = editTimerNoteDraft(cleanDraft(), "unfinished local note");

    expect(
      syncTimerNoteDraft(draft, {
        timerStartTime: "timer-1",
        note: "server note",
      }),
    ).toEqual(draft);
  });

  test("accepts a server update when the draft is clean", () => {
    expect(
      syncTimerNoteDraft(cleanDraft(), {
        timerStartTime: "timer-1",
        note: "updated elsewhere",
      }),
    ).toEqual({
      timerStartTime: "timer-1",
      note: "updated elsewhere",
      serverNote: "updated elsewhere",
    });
  });

  test("resets even a dirty draft when a new timer starts", () => {
    const draft = editTimerNoteDraft(cleanDraft(), "unfinished local note");

    expect(
      syncTimerNoteDraft(draft, {
        timerStartTime: "timer-2",
        note: "continuing my work",
      }),
    ).toEqual({
      timerStartTime: "timer-2",
      note: "continuing my work",
      serverNote: "continuing my work",
    });
  });

  test("keeps a newer local edit dirty when an earlier edit succeeds", () => {
    const savingDraft = editTimerNoteDraft(cleanDraft(), "first local note");
    const newerDraft = editTimerNoteDraft(savingDraft, "newer local note");
    const confirmedDraft = confirmTimerNoteDraftSaved(newerDraft, {
      timerStartTime: "timer-1",
      note: "first local note",
    });

    expect(confirmedDraft).toEqual({
      timerStartTime: "timer-1",
      note: "newer local note",
      serverNote: "first local note",
    });
    expect(isTimerNoteDraftDirty(confirmedDraft)).toBe(true);
  });
});
