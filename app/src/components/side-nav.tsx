import * as React from "react";
import {
  LucideIcon,
  FolderGit2,
  GitPullRequest,
  TimerIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { Link, LinkProps } from "@tanstack/react-router";
import { ScrollArea } from "./ui/scroll-area";
import { router } from "@/main";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { useQuery } from "@tanstack/react-query";
import { userQueries } from "@/lib/api/queries/user";
import { NotificationsPopover } from "./notifications-popover/notifications-popover";
import { ThemeSwitcher } from "./theme-switcher";

type LinkDestination = LinkProps<typeof router>["to"];
const MENU_ITEMS = [
  {
    title: "Pull requests",
    icon: GitPullRequest,
    variant: "ghost",
    to: "/prs",
  },
  {
    title: "Milltime",
    icon: TimerIcon,
    variant: "ghost",
    to: "/milltime",
  },
  {
    title: "Repositories",
    icon: FolderGit2,
    variant: "ghost",
    to: "/repositories",
  },
] as const satisfies readonly {
  title: string;
  icon: LucideIcon;
  variant: "default" | "ghost";
  to: LinkDestination;
}[];

type UsedLink = (typeof MENU_ITEMS)[number]["to"];

export function SideNavWrapper({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-col md:flex-row">
      {/* Navigation Bar */}
      <div className="fixed left-0 right-0 top-0 z-50 flex h-14 flex-row border-b border-border/50 bg-background/80 backdrop-blur-xl md:h-full md:w-[72px] md:flex-col md:items-stretch md:border-b-0 md:border-r md:bg-card/50">
        {/* Subtle glow effect at top */}
        <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-primary/20 to-transparent md:inset-y-0 md:left-0 md:h-full md:w-px md:bg-gradient-to-b" />

        <div className="flex w-full flex-row items-center justify-center gap-1 px-3 md:h-auto md:flex-col md:gap-2 md:px-0 md:py-4">
          {/* Logo/Avatar section */}
          <div className="flex h-full items-center justify-center md:mb-2 md:h-auto">
            <AvatarMenu />
          </div>

          {/* Notifications */}
          <div className="flex h-full items-center justify-center md:h-auto">
            <NotificationsPopover />
          </div>

          {/* Divider */}
          <div className="mx-2 h-6 w-px bg-border/50 md:mx-0 md:my-3 md:h-px md:w-8" />

          {/* Navigation items */}
          <Nav links={MENU_ITEMS} />

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
      <div className="mt-14 flex-1 md:ml-[72px] md:mt-0">
        <ScrollArea className="h-[calc(100vh-3.5rem)] md:h-screen [&>div>div]:!block">
          {children}
        </ScrollArea>
      </div>
    </div>
  );
}

function Nav({
  links,
}: {
  links: readonly {
    title: string;
    label?: string;
    icon: LucideIcon;
    variant: "default" | "ghost";
    to: UsedLink;
  }[];
}) {
  return (
    <div className="flex h-full items-center md:h-auto md:flex-col md:gap-1">
      <nav className="flex flex-row items-center gap-1 md:flex-col md:gap-1">
        {links.map((link, index) => (
          <NavLink
            key={index}
            title={link.title}
            label={link.label}
            link={{
              icon: link.icon,
              to: link.to,
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
  link,
}: {
  title: string;
  label?: string;
  link: {
    icon: LucideIcon;
    to: UsedLink;
  };
}) {
  return (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <Link
          to={link.to}
          className={cn(
            "group relative flex h-10 w-10 items-center justify-center rounded-xl transition-all duration-300",
            "hover:bg-primary/10"
          )}
          activeOptions={{ exact: true, includeSearch: false }}
          activeProps={{
            className: cn(
              "bg-primary/15 text-primary",
              "before:absolute before:inset-0 before:rounded-xl before:ring-1 before:ring-primary/20",
              "after:absolute after:-left-[1px] after:top-1/2 after:hidden after:h-6 after:w-[3px] after:-translate-y-1/2 after:rounded-r-full after:bg-primary md:after:block"
            ),
          }}
          inactiveProps={{
            className:
              "text-muted-foreground hover:text-foreground",
          }}
        >
          <link.icon className="h-5 w-5 transition-transform duration-300 group-hover:scale-110" />
          <span className="sr-only">{title}</span>
        </Link>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        className="flex items-center gap-3 rounded-lg border-border/50 bg-card/95 px-3 py-2 font-medium shadow-elevated backdrop-blur-sm"
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

function AvatarMenu() {
  const { data: me } = useQuery({
    ...userQueries.me(),
    staleTime: Infinity,
  });

  const avatarNumber = React.useMemo(
    () => Math.floor(Math.random() * 649 + 1),
    []
  );
  const avatarUrl = `https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/${avatarNumber}.png`;

  const initials = me?.fullName
    ?.split(" ")
    .map((n) => n[0])
    .join("");

  return (
    <div className="group relative">
      {/* Glow effect behind avatar */}
      <div className="absolute inset-0 rounded-full bg-primary/20 opacity-0 blur-md transition-opacity duration-300 group-hover:opacity-100" />
      <Avatar className="relative h-10 w-10 rounded-xl border border-border/50 bg-card shadow-sm transition-all duration-300 group-hover:border-primary/30 group-hover:shadow-glow-sm">
        <AvatarImage src={avatarUrl} className="rounded-xl" />
        <AvatarFallback className="rounded-xl bg-gradient-to-br from-primary/20 to-primary/5 text-sm font-semibold">
          {initials}
        </AvatarFallback>
      </Avatar>
    </div>
  );
}
