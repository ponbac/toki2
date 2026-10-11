import * as React from "react";
import { apiErrorMessage } from "@/lib/api/errors";

/**
 * The server's message for a failed request, once it has been read from the
 * response, or null while it is read or when there is none.
 */
export function useApiErrorMessage(error: unknown): string | null {
  const [message, setMessage] = React.useState<string | null>(null);
  React.useEffect(() => {
    let current = true;
    void apiErrorMessage(error, "").then((text) => {
      if (current) {
        setMessage(text || null);
      }
    });
    return () => {
      current = false;
    };
  }, [error]);
  return message;
}
