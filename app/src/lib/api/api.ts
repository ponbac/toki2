import { useMilltimeStore } from "@/hooks/useMilltimeStore";
import { router } from "@/main";
import ky, { KyResponse } from "ky";

const API_URL =
  import.meta.env.MODE === "development"
    ? "http://localhost:8080"
    : "https://toki2-api.ponbac.xyz";

import { z } from "zod";

const MilltimeErrorSchema = z.enum([
  "MilltimeAuthenticationFailed",
  "TimerError",
  "DateParseError",
  "FetchError",
]);

const ErrorResponseSchema = z.object({
  error: MilltimeErrorSchema,
  message: z.string(),
});

export type MilltimeError = z.infer<typeof MilltimeErrorSchema>;
export type ErrorResponse = z.infer<typeof ErrorResponseSchema>;

export const api = ky.create({
  prefixUrl: API_URL,
  credentials: "include",
  retry: 0,
  hooks: {
    afterResponse: [
      async (_, __, response) => {
        if (response.status === 401) {
          const parsedError = await parseResponseError(response);
          // Milltime authentication failed should not redirect to login page
          if (parsedError.data?.error === "MilltimeAuthenticationFailed") {
            useMilltimeStore.getState().actions.setIsAuthenticated(false);
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

async function parseResponseError(response: KyResponse) {
  try {
    const body = await response.text();
    const jsonBody = JSON.parse(body);
    return ErrorResponseSchema.safeParse(jsonBody);
  } catch (error) {
    console.debug("Failed to parse response body as error:", error);
    return { success: false, data: undefined };
  }
}
