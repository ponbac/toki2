import { HTTPError } from "ky";
import { toast } from "sonner";
import { z } from "zod";

const errorBodySchema = z.object({ error: z.string().optional() });

/** Returns the provider's parsed error message, or the caller's safe fallback. */
export async function apiErrorMessage(
  error: unknown,
  fallback: string,
): Promise<string> {
  if (!(error instanceof HTTPError)) {
    return fallback;
  }

  const body = errorBodySchema.safeParse(error.data);
  return body.success ? body.data.error?.trim() || fallback : fallback;
}

/** Displays a toast using the HTTP error body when it contains a message. */
export async function showApiErrorToast(
  error: unknown,
  fallback: string,
): Promise<void> {
  toast.error(await apiErrorMessage(error, fallback));
}

/** Builds an error callback for a mutation or detached promise notification. */
export function apiErrorToast(fallback: string): (error: unknown) => void {
  return (error) => {
    void showApiErrorToast(error, fallback);
  };
}
