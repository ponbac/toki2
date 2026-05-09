import { HTTPError } from "ky";
import { toast } from "sonner";

type ErrorBody = {
  error?: string;
};

export async function apiErrorMessage(
  error: unknown,
  fallback: string,
): Promise<string> {
  if (!(error instanceof HTTPError)) {
    return fallback;
  }

  try {
    const body = (await error.response.clone().json()) as ErrorBody;
    return body.error?.trim() || fallback;
  } catch {
    return fallback;
  }
}

export async function showApiErrorToast(
  error: unknown,
  fallback: string,
): Promise<void> {
  toast.error(await apiErrorMessage(error, fallback));
}

export function apiErrorToast(fallback: string): (error: unknown) => void {
  return (error) => {
    void showApiErrorToast(error, fallback);
  };
}
