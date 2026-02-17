import { useTimeTrackingStore } from "@/hooks/useTimeTrackingStore";
import { router } from "@/main";
import ky from "ky";

export const API_URL =
  import.meta.env.MODE === "development"
    ? "http://localhost:8080"
    : "https://toki-api.spinit.se";

export const api = ky.create({
  prefixUrl: API_URL,
  credentials: "include",
  retry: 0,
  hooks: {
    afterResponse: [
      async (_, __, response) => {
        if (response.status === 401) {
          const body = await response.clone().json().catch(() => null);
          if (body?.code === "TIME_TRACKING_AUTHENTICATION_FAILED") {
            useTimeTrackingStore.getState().actions.setIsAuthenticated(false);
            return response;
          }

          window.location.replace(
            `/login?next=${router.history.location.pathname}`,
          );
        }

        return response;
      },
    ],
  },
});
