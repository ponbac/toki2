import { CmdK } from "@/components/cmd-k";
import { LoadingSpinner } from "@/components/loading-spinner";
import { TimeTrackingLoginDialog } from "@/components/time-tracking-login-dialog";
import { FloatingTimer } from "@/components/floating-timer";
import { SideNavWrapper } from "@/components/side-nav";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { Suspense } from "react";
import { TimerEditDialog } from "@/components/timer-edit-dialog";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { useQuery } from "@tanstack/react-query";
import {
  useTimeTrackingActions,
  useTimeTrackingEditTimerDialogOpen,
  useTimeTrackingIsAuthenticated,
  useTimeTrackingLoginDialogOpen,
} from "@/hooks/useTimeTrackingStore";

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
      <Toaster
        position="top-right"
        closeButton
        toastOptions={{
          classNames: {
            closeButton:
              "!bg-background text-muted-foreground hover:text-primary transition-colors !border-muted-foreground hover:!border-primary",
          },
        }}
      />
      <CmdK />
      <TimerProvider />
    </TooltipProvider>
  );
}

function TimerProvider() {
  const isAuthenticated = useTimeTrackingIsAuthenticated();

  const editTimerDialogOpen = useTimeTrackingEditTimerDialogOpen();
  const loginDialogOpen = useTimeTrackingLoginDialogOpen();
  const { setLoginDialogOpen, setEditTimerDialogOpen } =
    useTimeTrackingActions();

  const { data: timerResponse } = useQuery({
    ...timeTrackingQueries.getTimer(),
    enabled: false,
  });
  const timer = timerResponse?.timer;

  return isAuthenticated ? (
    <>
      <FloatingTimer />
      {!!timer && (
        <TimerEditDialog
          open={editTimerDialogOpen}
          onOpenChange={setEditTimerDialogOpen}
          timer={timer}
        />
      )}
    </>
  ) : (
    <TimeTrackingLoginDialog
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
