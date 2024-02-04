import { LoadingSpinner } from "@/components/loading-spinner";
import { SideNavWrapper } from "@/components/side-nav";
import { Toaster } from "@/components/ui/sonner";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { Suspense } from "react";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  return (
    <>
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
        className="flex h-full min-h-screen w-full flex-col py-8"
      >
        <Suspense fallback={<FullscreenLoading />}>
          <Outlet />
        </Suspense>
      </SideNavWrapper>
      <Toaster />
    </>
  );
}

function FullscreenLoading() {
  return (
    <div className="flex min-h-screen w-full items-center justify-center">
      <LoadingSpinner className="size-8" />
    </div>
  );
}
