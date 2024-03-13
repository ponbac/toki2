import { router } from "@/main";
import ky from "ky";

export const api = ky.create({
  prefixUrl: "http://localhost:8080/",
  // prefixUrl: "https://toki2-api.ponbac.xyz",
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

export const login = async (next?: string) => {
  const authUrl = await api
    .post("login", {
      searchParams: next
        ? {
            next,
          }
        : undefined,
    })
    .text();

  window.location.href = authUrl;
};
