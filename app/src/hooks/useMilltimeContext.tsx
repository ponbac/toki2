/* eslint-disable react-refresh/only-export-components */
import { milltimeMutations } from "@/lib/api/mutations/milltime";
import React from "react";
import { type StoreApi, create, useStore } from "zustand";

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
  };
};

const MilltimeStoreContext = React.createContext<
  StoreApi<MilltimeStore> | undefined
>(undefined);

export const MilltimeStoreProvider = ({
  children,
}: {
  children: React.ReactNode;
}) => {
  const { mutate: authenticate } = milltimeMutations.useAuthenticate();

  const [store] = React.useState(() =>
    create<MilltimeStore>()((set) => ({
      isAuthenticated: isMilltimeCookiesPresent(),
      isAuthenticating: false,
      timer: {
        visible: false,
        state: undefined,
        timeSeconds: null,
      },
      newTimerDialogOpen: false,
      loginDialogOpen: false,
      actions: {
        authenticate: (credentials, onSuccess) => {
          set({ isAuthenticating: true });
          authenticate(
            {
              username: credentials.username,
              password: credentials.password,
            },
            {
              onSuccess: () => {
                set({ isAuthenticated: true });
                onSuccess?.();
              },
              onError: () => {
                set({ isAuthenticated: false });
                clearMilltimeCookies();
              },
              onSettled: () => {
                set({ isAuthenticating: false });
              },
            },
          );
        },
        reset: () => {
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
        },
        setTimer: (timer) =>
          set((state) => ({
            timer: {
              ...state.timer,
              ...timer,
            },
          })),
        setNewTimerDialogOpen: (open) => {
          set({ newTimerDialogOpen: open });
        },
        setLoginDialogOpen: (open) => {
          set({ loginDialogOpen: open });
        },
      },
    })),
  );

  return (
    <MilltimeStoreContext.Provider value={store}>
      {children}
    </MilltimeStoreContext.Provider>
  );
};

const useMilltimeStore = <T,>(selector: (state: MilltimeStore) => T) => {
  const store = React.useContext(MilltimeStoreContext);

  if (!store) {
    throw new Error("Missing MilltimeStoreContextProvider");
  }

  return useStore(store, selector);
};

export const useMilltimeIsAuthenticated = () =>
  useMilltimeStore((state) => state.isAuthenticated);
export const useMilltimeIsAuthenticating = () =>
  useMilltimeStore((state) => state.isAuthenticating);
export const useMilltimeTimer = () => useMilltimeStore((state) => state.timer);
export const useMilltimeNewTimerDialogOpen = () =>
  useMilltimeStore((state) => state.newTimerDialogOpen);
export const useMilltimeLoginDialogOpen = () =>
  useMilltimeStore((state) => state.loginDialogOpen);
export const useMilltimeActions = () =>
  useMilltimeStore((state) => state.actions);

function isMilltimeCookiesPresent() {
  return document.cookie.includes("mt_milltimesessionid");
}

function clearMilltimeCookies() {
  document.cookie = "mt_user=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_password=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_milltimesessionid=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
  document.cookie =
    "mt_CSRFToken=; expires=Thu, 01 Jan 1970 00:00:00 UTC; path=/;";
}
