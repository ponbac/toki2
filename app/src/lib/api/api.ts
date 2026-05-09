import ky from "ky";

const defaultApiUrl = import.meta.env.DEV
  ? "http://localhost:8180"
  : "https://toki-api.bkmn.xyz";

export const API_URL =
  import.meta.env.VITE_API_URL?.trim() || defaultApiUrl;

export const api = ky.create({
  prefixUrl: API_URL,
  credentials: "include",
  retry: 0,
  hooks: {
    afterResponse: [
      (_, __, response) => {
        if (response.status === 401) {
          window.location.replace(`/login?next=${window.location.pathname}`);
        }

        return response;
      },
    ],
  },
});
