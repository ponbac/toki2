import { HTTPError } from "ky";

type ApiErrorBody = {
  error?: string;
  code?: string;
};

export const TIME_TRACKING_PERIOD_LOCKED_CODE = "TIME_TRACKING_PERIOD_LOCKED";
const LOCKED_PERIOD_SAVE_TIMER_DESCRIPTION =
  "The selected day is in a locked period in Milltime. Unlock it or save to a different date.";

export async function getApiErrorDetails(error: unknown): Promise<{
  message: string;
  code?: string;
}> {
  if (error instanceof HTTPError) {
    const body = await error.response
      .clone()
      .json()
      .then((value) => value as ApiErrorBody)
      .catch(() => null);

    if (body?.error) {
      return { message: body.error, code: body.code };
    }

    return {
      message: `${error.response.status} ${error.response.statusText}`.trim(),
    };
  }

  if (error instanceof Error) {
    return { message: error.message };
  }

  return { message: "Unknown error" };
}

export async function getSaveTimerErrorDescription(error: unknown): Promise<string> {
  const { message, code } = await getApiErrorDetails(error);
  if (code === TIME_TRACKING_PERIOD_LOCKED_CODE) {
    return LOCKED_PERIOD_SAVE_TIMER_DESCRIPTION;
  }

  return message;
}
