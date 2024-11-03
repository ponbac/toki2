import { CmdK } from "@/components/cmd-k";
import { LoadingSpinner } from "@/components/loading-spinner";
import { MilltimeLoginDialog } from "@/components/milltime-login-dialog";
import { FloatingMilltimeTimer } from "@/components/floating-milltime-timer";
import { MilltimeTimerDialog } from "@/components/milltime-timer-dialog";
import { SideNavWrapper } from "@/components/side-nav";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { Suspense } from "react";
import { TimerEditDialog } from "@/components/timer-edit-dialog";
import { milltimeQueries } from "@/lib/api/queries/milltime";
import { useQuery } from "@tanstack/react-query";
import {
  useMilltimeActions,
  useMilltimeEditTimerDialogOpen,
  useMilltimeIsAuthenticated,
  useMilltimeLoginDialogOpen,
  useMilltimeNewTimerDialogOpen,
} from "@/hooks/useMilltimeStore";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  return (
    <TooltipProvider delayDuration={0}>
      <SideNavWrapper>
        <Suspense fallback={<FullscreenLoading />}>
          <Outlet />
        </Suspense>
      </SideNavWrapper>
      <Toaster position="top-right" />
      <CmdK />
      <MilltimeTimerProvider />
    </TooltipProvider>
  );
}

function MilltimeTimerProvider() {
  const isAuthenticated = useMilltimeIsAuthenticated();

  const newTimerDialogOpen = useMilltimeNewTimerDialogOpen();
  const editTimerDialogOpen = useMilltimeEditTimerDialogOpen();
  const loginDialogOpen = useMilltimeLoginDialogOpen();
  const { setNewTimerDialogOpen, setLoginDialogOpen, setEditTimerDialogOpen } =
    useMilltimeActions();

  const { data: timer } = useQuery({
    ...milltimeQueries.getTimer(),
    enabled: false,
  });

  return isAuthenticated ? (
    <>
      <FloatingMilltimeTimer />
      <MilltimeTimerDialog
        open={newTimerDialogOpen}
        onOpenChange={setNewTimerDialogOpen}
      />
      {!!timer && timer.timerType === "Standalone" && (
        <TimerEditDialog
          open={editTimerDialogOpen}
          onOpenChange={setEditTimerDialogOpen}
          timer={timer}
        />
      )}
    </>
  ) : (
    <MilltimeLoginDialog
      open={loginDialogOpen}
      onOpenChange={setLoginDialogOpen}
    />
  );
}

function FullscreenLoading() {
  return (
    <div className="flex min-h-screen w-full items-center justify-center">
      <LoadingSpinner className="size-8" />
    </div>
  );
}
