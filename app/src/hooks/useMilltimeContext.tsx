import { milltimeMutations } from "@/lib/api/mutations/milltime";
import React from "react";
import { type StoreApi, create, useStore } from "zustand";

type MilltimeStore = {
  isAuthenticated: boolean;
  isAuthenticating: boolean;
  timerVisible: boolean;
  actions: {
    authenticate: (credentials: { username: string; password: string }) => void;
    reset: () => void;
    setTimerVisible: (visible: boolean) => void;
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
      timerVisible: false,
      actions: {
        authenticate: (credentials) => {
          set({ isAuthenticating: true });
          authenticate(
            {
              username: credentials.username,
              password: credentials.password,
            },
            {
              onSuccess: () => {
                set({ isAuthenticated: true });
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
          set({ isAuthenticated: false, isAuthenticating: false });
          clearMilltimeCookies();
        },
        setTimerVisible: (visible) => {
          set({ timerVisible: visible });
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
export const useMilltimeIsTimerVisible = () =>
  useMilltimeStore((state) => state.timerVisible);
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