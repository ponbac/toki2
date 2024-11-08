import { api } from "@/lib/api/api";
import { create } from "zustand";

type Timer = {
  visible: boolean;
  state: "running" | "stopped" | undefined;
  timeSeconds: number | null;
};

type MilltimeStore = {
  isAuthenticated: boolean;
  isAuthenticating: boolean;
  timer: Timer;
  newTimerDialogOpen: boolean;
  editTimerDialogOpen: boolean;
  loginDialogOpen: boolean;
  actions: {
    authenticate: (
      credentials: { username: string; password: string },
      onSuccess?: () => void,
    ) => void;
    reset: () => void;
    setTimer: (timer: Partial<Timer>) => void;
    setNewTimerDialogOpen: (open: boolean) => void;
    setLoginDialogOpen: (open: boolean) => void;
    setEditTimerDialogOpen: (open: boolean) => void;
  };
};

function isMilltimeCookiesPresent() {
  return (
    document.cookie.includes("mt_milltimesessionid") ||
    document.cookie.includes("mt_user")
  );
}

export function clearMilltimeCookies() {
  document.cookie = "mt_user=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_password=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_milltimesessionid=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_CSRFToken=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
}

export const useMilltimeStore = create<MilltimeStore>()((set) => ({
  isAuthenticated: isMilltimeCookiesPresent(),
  isAuthenticating: false,
  timer: {
    visible: false,
    state: undefined,
    timeSeconds: null,
  },
  newTimerDialogOpen: false,
  editTimerDialogOpen: false,
  loginDialogOpen: false,
  actions: {
    authenticate: (
      credentials: { username: string; password: string },
      onSuccess?: () => void,
    ) => {
      set({ isAuthenticating: true });
      api
        .post("milltime/authenticate", {
          json: credentials,
        })
        .then((res) => {
          if (res.ok) {
            set({ isAuthenticated: true });
            onSuccess?.();
          }
        })
        .catch(() => {
          set({ isAuthenticated: false });
          clearMilltimeCookies();
        })
        .finally(() => {
          set({ isAuthenticating: false });
        });
    },

    reset: () => {
      if (isMilltimeCookiesPresent()) {
        set({
          isAuthenticated: false,
          isAuthenticating: false,
          timer: {
            visible: false,
            state: undefined,
            timeSeconds: null,
          },
        });
        clearMilltimeCookies();
      }
    },

    setTimer: (timer: Partial<Timer>) =>
      set((state) => ({
        timer: {
          ...state.timer,
          ...timer,
        },
      })),

    setNewTimerDialogOpen: (open: boolean) => set({ newTimerDialogOpen: open }),
    setLoginDialogOpen: (open: boolean) => set({ loginDialogOpen: open }),
    setEditTimerDialogOpen: (open: boolean) =>
      set({ editTimerDialogOpen: open }),
  },
}));

// Selector hooks for convenience
export const useMilltimeIsAuthenticated = () =>
  useMilltimeStore((state) => state.isAuthenticated);
export const useMilltimeIsAuthenticating = () =>
  useMilltimeStore((state) => state.isAuthenticating);
export const useMilltimeTimer = () => useMilltimeStore((state) => state.timer);
export const useMilltimeNewTimerDialogOpen = () =>
  useMilltimeStore((state) => state.newTimerDialogOpen);
export const useMilltimeLoginDialogOpen = () =>
  useMilltimeStore((state) => state.loginDialogOpen);
export const useMilltimeEditTimerDialogOpen = () =>
  useMilltimeStore((state) => state.editTimerDialogOpen);
export const useMilltimeActions = () =>
  useMilltimeStore((state) => state.actions);
