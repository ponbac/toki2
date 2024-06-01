import { useMilltimeIsTimerVisible } from "@/hooks/useMilltimeContext";
import { useMilltimeData } from "@/hooks/useMilltimeData";
import { MilltimeTimerDialog } from "./milltime-timer-dialog";
import React from "react";
import { Button } from "./ui/button";
import {
  Maximize2Icon,
  Minimize2Icon,
  PauseIcon,
  PlayIcon,
} from "lucide-react";
import { Input } from "./ui/input";
import { cn } from "@/lib/utils";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";

export const MilltimeTimer = () => {
  const { isAuthenticated } = useMilltimeData();
  const visible = useMilltimeIsTimerVisible();

  const [dialogOpen, setDialogOpen] = React.useState(false);
  const [isMinimized, setIsMinimized] = React.useState(false);

  const { data: timer } = useQuery({
    ...milltimeQueries.getTimer(),
    refetchInterval: 5000,
  });

  return !visible ? (
    <>
      <div
        className={cn(
          "fixed right-4 top-4 w-[340px] rounded-lg bg-white p-4 shadow-lg dark:bg-gray-900",
          {
            "w-fit px-2 py-1": isMinimized,
          },
        )}
      >
        {isAuthenticated ? (
          <div className="flex flex-col items-center justify-between space-y-4">
            <div className="flex w-full items-center justify-between gap-2">
              <div
                className={cn(
                  "text-4xl font-bold tracking-tighter text-gray-900 dark:text-gray-50",
                  {
                    "text-2xl": isMinimized,
                  },
                )}
              >
                {String(timer?.hours ?? 0).padStart(2, "0")}:
                {String(timer?.minutes ?? 0).padStart(2, "0")}:
                {String(timer?.seconds ?? 0).padStart(2, "0")}
              </div>
              <div
                className={cn("flex items-center space-x-2", {
                  hidden: isMinimized,
                })}
              >
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setDialogOpen(true)}
                >
                  <PlayIcon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                  <span className="sr-only">Start</span>
                </Button>
                <Button variant="ghost" size="icon">
                  <PauseIcon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                  <span className="sr-only">Stop</span>
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setIsMinimized(true)}
                >
                  <Minimize2Icon className="h-6 w-6 text-gray-500 dark:text-gray-400" />
                  <span className="sr-only">Minimize</span>
                </Button>
              </div>
              <div
                className={cn("hidden", {
                  flex: isMinimized,
                })}
              >
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setIsMinimized(false)}
                >
                  <Maximize2Icon className="size-4 text-gray-500 dark:text-gray-400" />
                  <span className="sr-only">Maximize</span>
                </Button>
              </div>
            </div>
            <div
              className={cn("w-full", {
                hidden: isMinimized,
              })}
            >
              <Input
                type="text"
                placeholder="Add a note..."
                value={timer?.userNote}
                disabled
                className="w-full rounded-md border border-gray-300 px-4 py-2 text-gray-900 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-50"
              />
            </div>
          </div>
        ) : (
          <h1 className="text-balance">
            You need to be authenticated to Milltime to use the timer.
          </h1>
        )}
      </div>
      <MilltimeTimerDialog open={dialogOpen} onOpenChange={setDialogOpen} />
    </>
  ) : null;
};
