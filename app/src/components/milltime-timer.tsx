import { useMilltimeIsTimerVisible } from "@/hooks/useMilltimeContext";
import { useMilltimeData } from "@/hooks/useMilltimeData";

export const MilltimeTimer = () => {
  const { isAuthenticated } = useMilltimeData();

  const visible = useMilltimeIsTimerVisible();

  return !visible ? (
    <div className="absolute right-4 top-4 flex h-16 w-72 items-center justify-center rounded-3xl border-2 border-primary bg-popover">
      <h1>Milltime Timer</h1>
      {!isAuthenticated && <p>Not authenticated</p>}
    </div>
  ) : null;
};
