import * as React from "react";
import {
  LucideIcon,
  BrainCircuit,
  FolderGit2,
  GitPullRequest,
  KanbanSquare,
  ShieldCheck,
  TimerIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import {
  Link,
  LinkProps,
  useNavigate,
  useRouterState,
} from "@tanstack/react-router";
import { ScrollArea } from "./ui/scroll-area";
import { router } from "@/main";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { useDueLockAlert } from "@/lib/period-lock/use-lock-days";
import type { DueLock } from "@/lib/period-lock/lock-periods";
import { differsQueries } from "@/lib/api/queries/differs";
import { userQueries } from "@/lib/api/queries/user";
import { NotificationsPopover } from "./notifications-popover/notifications-popover";
import { ThemeSwitcher } from "./theme-switcher";
import { AccountSettings } from "./account-settings";

type LinkDestination = LinkProps<typeof router>["to"];
const MENU_ITEMS = [
  {
    title: "Pull requests",
    icon: GitPullRequest,
    variant: "ghost",
    to: "/prs",
  },
  {
    title: "Board",
    icon: KanbanSquare,
    variant: "ghost",
    to: "/board",
  },
  {
    title: "Time Tracking",
    icon: TimerIcon,
    variant: "ghost",
    to: "/time-tracking",
  },
  {
    title: "Repositories",
    icon: FolderGit2,
    variant: "ghost",
    to: "/repositories",
  },
  {
    title: "AI usage",
    icon: BrainCircuit,
    variant: "ghost",
    to: "/ai-usage",
  },
  {
    title: "Admin",
    icon: ShieldCheck,
    variant: "ghost",
    to: "/admin",
    adminOnly: true,
    matchNested: true,
  },
] as const satisfies readonly {
  title: string;
  icon: LucideIcon;
  variant: "default" | "ghost";
  to: LinkDestination;
  /** Shown only to admins. */
  adminOnly?: boolean;
  /** Active on nested routes too, such as the admin sections. */
  matchNested?: boolean;
}[];
const BOARD_MENU_ORGANIZATION = "lerumsdjur";

type UsedLink = (typeof MENU_ITEMS)[number]["to"];

const LOCK_REMINDER_KEY = "time-tracking-lock-reminded-on";

/** Overdue time gets one toast a day, wherever the person is working. */
function useOverdueLockReminder(due: DueLock | null) {
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isOverdue = due !== null && due.severity !== "due";
  const title = due?.title;

  React.useEffect(() => {
    if (!isOverdue || pathname.startsWith("/time-tracking")) return;

    const today = new Date().toDateString();
    if (localStorage.getItem(LOCK_REMINDER_KEY) === today) return;

    localStorage.setItem(LOCK_REMINDER_KEY, today);
    toast.warning(title, {
      action: {
        label: "Review",
        onClick: () => void navigate({ to: "/time-tracking" }),
      },
    });
  }, [isOverdue, pathname, title, navigate]);
}

export function SideNavWrapper({ children }: { children: React.ReactNode }) {
  const dueLock = useDueLockAlert();
  useOverdueLockReminder(dueLock);

  const { data: differs } = useQuery({
    ...differsQueries.differs(),
  });

  const canSeeBoard = React.useMemo(
    () =>
      differs?.some(
        (differ) =>
          differ.followed &&
          differ.organization.toLowerCase() === BOARD_MENU_ORGANIZATION,
      ) ?? false,
    [differs],
  );

  const { data: isAdmin = false } = useQuery({
    ...userQueries.me(),
    select: (me) => me.roles.includes("Admin"),
  });

  const menuItems = React.useMemo(
    () =>
      MENU_ITEMS.filter(
        (item) =>
          (item.to !== "/board" || canSeeBoard) &&
          (!("adminOnly" in item) || isAdmin),
      ),
    [canSeeBoard, isAdmin],
  );

  return (
    <div className="flex flex-col md:flex-row">
      {/* Navigation Bar */}
      <div className="fixed left-0 right-0 top-0 z-50 flex h-14 flex-row border-b border-border/50 bg-background/80 backdrop-blur-xl md:h-full md:w-[72px] md:flex-col md:items-stretch md:border-b-0 md:border-r md:bg-card/50">
        {/* Subtle glow effect at top */}
        <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-linear-to-r from-transparent via-primary/20 to-transparent md:inset-y-0 md:left-0 md:h-full md:w-px md:bg-linear-to-b" />

        <div className="flex w-full flex-row items-center justify-center gap-1 px-3 md:h-auto md:flex-col md:gap-2 md:px-0 md:py-4">
          {/* Logo/Avatar section */}
          <div className="flex h-full items-center justify-center md:mb-2 md:h-auto">
            <AccountSettings />
          </div>

          {/* Notifications */}
          <div className="flex h-full items-center justify-center md:h-auto">
            <NotificationsPopover />
          </div>

          {/* Divider */}
          <div className="mx-2 h-6 w-px bg-border/50 md:mx-0 md:my-3 md:h-px md:w-8" />

          {/* Navigation items */}
          <Nav links={menuItems} dueLock={dueLock} />

          {/* Theme switcher - inline on mobile */}
          <div className="flex h-full items-center justify-center md:hidden">
            <div className="mx-1 h-6 w-px bg-border/50" />
            <ThemeSwitcher />
          </div>
        </div>

        {/* Theme switcher at bottom - desktop */}
        <div className="absolute bottom-4 left-1/2 hidden -translate-x-1/2 md:block">
          <ThemeSwitcher />
        </div>
      </div>

      {/* Main content area */}
      <div className="mt-14 min-w-0 flex-1 md:ml-[72px] md:mt-0">
        <ScrollArea className="h-[calc(100vh-3.5rem)] md:h-screen [&>div>div]:!block">
          {children}
        </ScrollArea>
      </div>
    </div>
  );
}

function Nav({
  links,
  dueLock,
}: {
  dueLock: DueLock | null;
  links: readonly {
    title: string;
    label?: string;
    icon: LucideIcon;
    variant: "default" | "ghost";
    to: UsedLink;
    matchNested?: boolean;
  }[];
}) {
  return (
    <div className="flex h-full items-center md:h-auto md:flex-col md:gap-1">
      <nav className="flex flex-row items-center gap-1 md:flex-col md:gap-1">
        {links.map((link, index) => (
          <NavLink
            key={index}
            title={link.title}
            label={link.to === "/time-tracking" ? dueLock?.title : link.label}
            alert={link.to === "/time-tracking" ? dueLock?.severity : undefined}
            link={{
              icon: link.icon,
              to: link.to,
              matchNested: link.matchNested,
            }}
          />
        ))}
      </nav>
    </div>
  );
}

function NavLink({
  title,
  label,
  alert,
  link,
}: {
  title: string;
  label?: string;
  alert?: DueLock["severity"];
  link: {
    icon: LucideIcon;
    to: UsedLink;
    matchNested?: boolean;
  };
}) {
  return (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <Link
          to={link.to}
          className={cn(
            "group relative flex h-10 w-10 items-center justify-center rounded-xl transition-all duration-300",
            "hover:bg-primary/10",
          )}
          activeOptions={{ exact: !link.matchNested, includeSearch: false }}
          activeProps={{
            className: cn(
              "bg-primary/15 text-primary",
              "before:absolute before:inset-0 before:rounded-xl before:ring-1 before:ring-primary/20",
              "after:absolute after:-left-[1px] after:top-1/2 after:hidden after:h-6 after:w-[3px] after:-translate-y-1/2 after:rounded-r-full after:bg-primary md:after:block",
            ),
          }}
          inactiveProps={{
            className: "text-muted-foreground hover:text-foreground",
          }}
        >
          <link.icon className="h-5 w-5 transition-transform duration-300 group-hover:scale-110" />
          <span className="sr-only">
            {title}
            {alert ? `, ${label}` : ""}
          </span>
          {alert && (
            <span
              className={cn(
                "absolute right-1.5 top-1.5 h-2.5 w-2.5 rounded-full ring-2 ring-background",
                alert === "month-overdue" ? "bg-destructive" : "bg-primary",
              )}
            >
              <span className="absolute inset-0 rounded-full bg-inherit motion-safe:animate-[ping_1.2s_ease-out_2]" />
            </span>
          )}
        </Link>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        className="flex items-center gap-3 rounded-lg border-border/50 bg-card/95 px-3 py-2 font-medium shadow-elevated backdrop-blur-xs"
      >
        {title}
        {label && (
          <span className="rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
            {label}
          </span>
        )}
      </TooltipContent>
    </Tooltip>
  );
}
