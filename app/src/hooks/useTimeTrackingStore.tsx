import { api } from "@/lib/api/api";
import { Link } from "@tanstack/react-router";
import { toast } from "sonner";
import { create } from "zustand";

type Timer = {
  visible: boolean;
  state: "running" | "stopped" | undefined;
  timeSeconds: number | null;
};

type TimeTrackingStore = {
  isAuthenticated: boolean;
  isAuthenticating: boolean;
  initialAuthChecked: boolean;
  timer: Timer;
  editTimerDialogOpen: boolean;
  loginDialogOpen: boolean;
  actions: {
    authenticate: (
      credentials: { username: string; password: string },
      onSuccess?: () => void,
    ) => void;
    reset: () => void;
    setTimer: (timer: Partial<Timer>) => void;
    setLoginDialogOpen: (open: boolean) => void;
    setEditTimerDialogOpen: (open: boolean) => void;
    setIsAuthenticated: (isAuthenticated: boolean) => void;
  };
};

function isProviderCookiesPresent() {
  return (
    document.cookie.includes("mt_milltimesessionid") ||
    document.cookie.includes("mt_user")
  );
}

export function clearProviderCookies() {
  console.debug("Clearing provider cookies");

  const domains = [".spinit.se", location.hostname];
  const cookies = [
    "mt_user",
    "mt_password",
    "mt_milltimesessionid",
    "mt_CSRFToken",
  ];

  domains.forEach((domain) => {
    cookies.forEach((cookie) => {
      document.cookie = `${cookie}=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/; domain=${domain}`;
    });
  });
}

export const useTimeTrackingStore = create<TimeTrackingStore>()((set, get) => ({
  isAuthenticated: isProviderCookiesPresent(),
  isAuthenticating: false,
  initialAuthChecked: false,
  timer: {
    visible: false,
    state: undefined,
    timeSeconds: null,
  },
  editTimerDialogOpen: false,
  loginDialogOpen: false,
  actions: {
    authenticate: (
      credentials: { username: string; password: string },
      onSuccess?: () => void,
    ) => {
      set({ isAuthenticating: true });
      api
        .post("time-tracking/authenticate", {
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
          clearProviderCookies();
        })
        .finally(() => {
          set({ isAuthenticating: false, initialAuthChecked: true });
        });
    },
    reset: () => {
      if (isProviderCookiesPresent()) {
        set({
          isAuthenticated: false,
          isAuthenticating: false,
          timer: {
            visible: false,
            state: undefined,
            timeSeconds: null,
          },
        });
        clearProviderCookies();
      }
    },
    setTimer: (timer: Partial<Timer>) =>
      set((state) => ({
        timer: {
          ...state.timer,
          ...timer,
        },
      })),
    setLoginDialogOpen: (open: boolean) => set({ loginDialogOpen: open }),
    setEditTimerDialogOpen: (open: boolean) =>
      set({ editTimerDialogOpen: open }),
    setIsAuthenticated: (newIsAuthenticated: boolean) => {
      // If the user is not authenticated and was previously authenticated
      if (!newIsAuthenticated && get().isAuthenticated) {
        clearProviderCookies();
        toast.error("Could not connect to Milltime", {
          description: "Please try signing in again.",
          duration: Infinity,
          dismissible: true,
          classNames: {
            toast: "!border-destructive",
          },
        });

        return set({
          isAuthenticated: false,
          timer: { visible: false, state: undefined, timeSeconds: null },
        });
      } else if (!get().initialAuthChecked && !newIsAuthenticated) {
        toast.info("Could not connect to Milltime", {
          // description: "Try going to the Milltime view and signing in.",
          description: (
            <p>
              Try going to the{" "}
              <Link
                className="font-bold underline transition-colors hover:text-primary"
                to="/time-tracking"
              >
                Time Tracking
              </Link>{" "}
              and signing in.
            </p>
          ),
          duration: Infinity,
          dismissible: true,
        });
      }

      return set({
        isAuthenticated: newIsAuthenticated,
        initialAuthChecked: true,
      });
    },
  },
}));

// Selector hooks for convenience
export const useTimeTrackingIsAuthenticated = () =>
  useTimeTrackingStore((state) => state.isAuthenticated);
export const useTimeTrackingIsAuthenticating = () =>
  useTimeTrackingStore((state) => state.isAuthenticating);
export const useTimeTrackingTimer = () => useTimeTrackingStore((state) => state.timer);
export const useTimeTrackingLoginDialogOpen = () =>
  useTimeTrackingStore((state) => state.loginDialogOpen);
export const useTimeTrackingEditTimerDialogOpen = () =>
  useTimeTrackingStore((state) => state.editTimerDialogOpen);
export const useTimeTrackingActions = () =>
  useTimeTrackingStore((state) => state.actions);
