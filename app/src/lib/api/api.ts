import ky from "ky";

const productionDefaultApiUrl = () => {
  if (window.location.hostname.startsWith("toki.")) {
    return `${window.location.protocol}//${window.location.hostname.replace(
      /^toki\./,
      "toki-api.",
    )}`;
  }

  return "https://toki-api.spinit.se";
};

const defaultApiUrl = import.meta.env.DEV
  ? "http://localhost:8180"
  : productionDefaultApiUrl();

export const API_URL = import.meta.env.VITE_API_URL?.trim() || defaultApiUrl;

/** Authenticated HTTP client with login redirection on 401 and no automatic retries. */
export const api = ky.create({
  prefix: API_URL,
  credentials: "include",
  retry: 0,
  timeout: 45_000,
  hooks: {
    afterResponse: [
      ({ response }) => {
        if (response.status === 401) {
          window.location.replace(`/login?next=${window.location.pathname}`);
        }

        return response;
      },
    ],
  },
});
