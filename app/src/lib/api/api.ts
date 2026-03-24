import ky from "ky";

export const API_URL =
  import.meta.env.MODE === "development"
    ? "http://localhost:8180"
    : "https://toki-api.spinit.se";

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
