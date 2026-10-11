import { Link, Outlet, createFileRoute } from "@tanstack/react-router";
import { useSuspenseQuery } from "@tanstack/react-query";
import { ShieldAlert } from "lucide-react";
import { userQueries } from "@/lib/api/queries/user";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/_layout/admin")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(userQueries.me()),
  component: AdminLayout,
});

const TABS = [
  { to: "/admin/ai-billing", label: "AI billing" },
  { to: "/admin/project-mappings", label: "Project mappings" },
  { to: "/admin/ai-subscriptions", label: "AI subscriptions" },
] as const;

function AdminLayout() {
  const { data: isAdmin } = useSuspenseQuery({
    ...userQueries.me(),
    select: (me) => me.roles.includes("Admin"),
  });

  if (!isAdmin) {
    return (
      <main className="flex min-h-[60vh] w-full items-center justify-center p-6">
        <div className="flex max-w-md flex-col items-center gap-3 text-center">
          <ShieldAlert className="size-8 text-muted-foreground" />
          <h1 className="text-xl font-semibold">Admins only</h1>
          <p className="text-sm text-muted-foreground">
            The admin area shows everyone&apos;s AI usage and billing. Ask an
            admin if you need something from it.
          </p>
        </div>
      </main>
    );
  }

  return (
    <main className="mx-auto flex w-full max-w-7xl flex-col gap-6 p-4 md:p-8">
      <header className="flex flex-col gap-4">
        <div>
          <h1 className="text-2xl font-bold">Admin</h1>
          <p className="text-sm text-muted-foreground">
            AI usage billing, and the mappings and subscriptions it depends on.
            USD figures are API-equivalent estimates, not provider bills.
          </p>
        </div>
        <nav
          className="flex gap-1 overflow-x-auto border-b border-border/60"
          aria-label="Admin sections"
        >
          {TABS.map((tab) => (
            <Link
              key={tab.to}
              to={tab.to}
              className={cn(
                "-mb-px whitespace-nowrap border-b-2 px-3 py-2 text-sm transition-colors",
              )}
              activeProps={{
                className: "border-primary font-semibold text-foreground",
              }}
              inactiveProps={{
                className:
                  "border-transparent text-muted-foreground hover:text-foreground",
              }}
            >
              {tab.label}
            </Link>
          ))}
        </nav>
      </header>
      <Outlet />
    </main>
  );
}
