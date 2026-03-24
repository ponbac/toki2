import { create } from "zustand";

type Timer = {
  visible: boolean;
  state: "running" | "stopped" | undefined;
  timeSeconds: number | null;
};

type TimeTrackingStore = {
  timer: Timer;
  editTimerDialogOpen: boolean;
  actions: {
    reset: () => void;
    setTimer: (timer: Partial<Timer>) => void;
    setEditTimerDialogOpen: (open: boolean) => void;
  };
};

export const useTimeTrackingStore = create<TimeTrackingStore>()((set) => ({
  timer: {
    visible: false,
    state: undefined,
    timeSeconds: null,
  },
  editTimerDialogOpen: false,
  actions: {
    reset: () => {
      set({
        timer: {
          visible: false,
          state: undefined,
          timeSeconds: null,
        },
      });
    },
    setTimer: (timer: Partial<Timer>) =>
      set((state) => ({
        timer: {
          ...state.timer,
          ...timer,
        },
      })),
    setEditTimerDialogOpen: (open: boolean) =>
      set({ editTimerDialogOpen: open }),
  },
}));

export const useTimeTrackingTimer = () =>
  useTimeTrackingStore((state) => state.timer);
export const useTimeTrackingEditTimerDialogOpen = () =>
  useTimeTrackingStore((state) => state.editTimerDialogOpen);
export const useTimeTrackingActions = () =>
  useTimeTrackingStore((state) => state.actions);
