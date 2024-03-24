import { router } from "@/main";
import ky from "ky";

const API_URL =
  import.meta.env.MODE === "development"
    ? "http://localhost:8080"
    : "https://toki2-api.ponbac.xyz";

export const api = ky.create({
  prefixUrl: API_URL,
  credentials: "include",
  hooks: {
    afterResponse: [
      (_, __, response) => {
        if (response.status === 401) {
          router.history.push(
            `/login?next=${router.history.location.pathname}`,
          );
        }

        return response;
      },
    ],
  },
});
