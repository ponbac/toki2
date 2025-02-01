import { api } from "@/lib/api/api";
import { Link } from "@tanstack/react-router";
import { toast } from "sonner";
import { create } from "zustand";

type Timer = {
  visible: boolean;
  state: "running" | "stopped" | undefined;
  timeSeconds: number | null;
};

type MilltimeStore = {
  isAuthenticated: boolean;
  isAuthenticating: boolean;
  initialAuthChecked: boolean;
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
    setIsAuthenticated: (isAuthenticated: boolean) => void;
  };
};

function isMilltimeCookiesPresent() {
  return (
    document.cookie.includes("mt_milltimesessionid") ||
    document.cookie.includes("mt_user")
  );
}

export function clearMilltimeCookies() {
  console.debug("Clearing milltime cookies");

  const domains = [".ponbac.xyz", location.hostname];
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

export const useMilltimeStore = create<MilltimeStore>()((set, get) => ({
  isAuthenticated: isMilltimeCookiesPresent(),
  isAuthenticating: false,
  initialAuthChecked: false,
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
          set({ isAuthenticating: false, initialAuthChecked: true });
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
    setIsAuthenticated: (newIsAuthenticated: boolean) => {
      // If the user is not authenticated and was previously authenticated
      if (!newIsAuthenticated && get().isAuthenticated) {
        clearMilltimeCookies();
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
                to="/milltime"
              >
                Milltime view
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
