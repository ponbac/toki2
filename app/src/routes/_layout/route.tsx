import { SideNavWrapper } from "@/components/side-nav";
import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  return (
    <SideNavWrapper
      accounts={[
        {
          email: "root@ponbac.xyz",
          label: "Root",
          icon: "👑",
        },
      ]}
      navCollapsedSize={2}
      defaultCollapsed={false}
      className="flex h-full min-h-screen w-full flex-col"
    >
      <Outlet />
    </SideNavWrapper>
  );
}
