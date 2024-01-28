import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  return (
    <div className="flex min-h-screen flex-row">
      <div className="flex h-screen flex-col bg-slate-500">Wabbado!</div>
      <Outlet />
    </div>
  );
}
