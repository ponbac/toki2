import { useEffect, useState } from "react";
import dayjs from "dayjs";

/** Today's local date as `YYYY-MM-DD`, moving on at midnight and when the tab
 * is shown again, so a page left open overnight asks about the new day. */
export function useLocalDate() {
  const [date, setDate] = useState(() => dayjs().format("YYYY-MM-DD"));

  useEffect(() => {
    const sync = () => setDate(dayjs().format("YYYY-MM-DD"));
    const now = dayjs();
    const timeout = setTimeout(
      sync,
      now.add(1, "day").startOf("day").diff(now) + 1000,
    );
    document.addEventListener("visibilitychange", sync);

    return () => {
      clearTimeout(timeout);
      document.removeEventListener("visibilitychange", sync);
    };
  }, [date]);

  return date;
}
