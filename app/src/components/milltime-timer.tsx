import { useMilltimeIsTimerVisible } from "@/hooks/useMilltimeContext";
import { useMilltimeData } from "@/hooks/useMilltimeData";
import { MilltimeTimerDialog } from "./milltime-timer-dialog";
import React from "react";
import { Button } from "./ui/button";
import { PlayCircleIcon } from "lucide-react";

export const MilltimeTimer = () => {
  const { isAuthenticated } = useMilltimeData();
  const visible = useMilltimeIsTimerVisible();

  const [dialogOpen, setDialogOpen] = React.useState(true);

  return !visible ? (
    <>
      <div className="absolute right-4 top-4 flex h-16 w-72 items-center justify-center rounded-3xl border-2 border-primary bg-popover">
        <h1>Milltime Timer</h1>
        <Button variant="ghost" onClick={() => setDialogOpen(true)}>
          <PlayCircleIcon />
        </Button>
        {!isAuthenticated && <p>Not authenticated</p>}
      </div>
      <MilltimeTimerDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  ) : null;
};
