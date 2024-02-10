import { CmdK } from "@/components/cmd-k";
import { LoadingSpinner } from "@/components/loading-spinner";
import { SideNavWrapper } from "@/components/side-nav";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { Suspense } from "react";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  return (
    <TooltipProvider delayDuration={0}>
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
    </TooltipProvider>
  );
}

function FullscreenLoading() {
  return (
    <div className="flex min-h-screen w-full items-center justify-center">
      <LoadingSpinner className="size-8" />
    </div>
  );
}
