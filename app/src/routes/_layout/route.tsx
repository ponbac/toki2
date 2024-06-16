import { CmdK } from "@/components/cmd-k";
import { LoadingSpinner } from "@/components/loading-spinner";
import { MilltimeLoginDialog } from "@/components/milltime-login-dialog";
import { MilltimeTimer } from "@/components/milltime-timer";
import { MilltimeTimerDialog } from "@/components/milltime-timer-dialog";
import { SideNavWrapper } from "@/components/side-nav";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  MilltimeStoreProvider,
  useMilltimeActions,
  useMilltimeIsAuthenticated,
  useMilltimeLoginDialogOpen,
  useMilltimeNewTimerDialogOpen,
} from "@/hooks/useMilltimeContext";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { Suspense } from "react";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  return (
    <TooltipProvider delayDuration={0}>
      <MilltimeStoreProvider>
        <SideNavWrapper
          accounts={[
            {
              email: "root@ponbac.xyz",
              label: "Root",
              icon: "👑",
            },
          ]}
          navCollapsedSize={2}
          defaultCollapsed={true}
          className="flex h-full min-h-screen w-full flex-col"
        >
          <Suspense fallback={<FullscreenLoading />}>
            <Outlet />
          </Suspense>
        </SideNavWrapper>
        <Toaster />
        <CmdK />
        <MilltimeTimerProvider />
      </MilltimeStoreProvider>
    </TooltipProvider>
  );
}

function MilltimeTimerProvider() {
  const isAuthenticated = useMilltimeIsAuthenticated();

  const newTimerDialogOpen = useMilltimeNewTimerDialogOpen();
  const loginDialogOpen = useMilltimeLoginDialogOpen();
  const { setNewTimerDialogOpen, setLoginDialogOpen } = useMilltimeActions();

  return isAuthenticated ? (
    <>
      <MilltimeTimer />
      <MilltimeTimerDialog
        open={newTimerDialogOpen}
        onOpenChange={setNewTimerDialogOpen}
      />
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
