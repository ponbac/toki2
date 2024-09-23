import * as React from "react";
import {
  LucideIcon,
  FolderGit2,
  GitPullRequest,
  TimerIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { Separator } from "./ui/separator";
import { buttonVariants } from "./ui/button";
import { Link, LinkProps } from "@tanstack/react-router";
import { ScrollArea } from "./ui/scroll-area";
import { router } from "@/main";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { useQuery } from "@tanstack/react-query";
import { userQueries } from "@/lib/api/queries/user";

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
    <div className="flex">
      <div className="fixed left-0 top-0 flex h-full w-14 flex-col items-stretch border-r">
        <div className="flex h-[52px] items-center justify-center">
          <AvatarMenu />
        </div>
        <Separator />
        <Nav isCollapsed={true} links={MENU_ITEMS} />
      </div>
      <div className="ml-16 flex-1">
        <ScrollArea className="h-screen">{children}</ScrollArea>
      </div>
    </div>
  );
}

export function Nav({
  links,
  isCollapsed,
}: {
  isCollapsed: boolean;
  links: readonly {
    title: string;
    label?: string;
    icon: LucideIcon;
    variant: "default" | "ghost";
    to: UsedLink;
  }[];
}) {
  return (
    <div
      data-collapsed={isCollapsed}
      className="group flex flex-col gap-4 py-2 data-[collapsed=true]:py-2"
    >
      <nav className="grid gap-1 px-2 group-[[data-collapsed=true]]:justify-center group-[[data-collapsed=true]]:px-2">
        {links.map((link, index) => (
          <NavLink
            key={index}
            title={link.title}
            label={link.label}
            link={{
              icon: link.icon,
              to: link.to,
            }}
            isCollapsed={isCollapsed}
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
  isCollapsed,
}: {
  title: string;
  label?: string;
  link: {
    icon: LucideIcon;
    to: UsedLink;
  };
  isCollapsed: boolean;
}) {
  return isCollapsed ? (
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <Link
          to={link.to}
          className={cn("h-9 w-9")}
          activeOptions={{ exact: true, includeSearch: false }}
          activeProps={{
            className: cn(
              buttonVariants({ variant: "default", size: "icon" }),
              "dark:bg-muted dark:text-muted-foreground dark:hover:bg-muted dark:hover:text-white",
            ),
          }}
          inactiveProps={{
            className: buttonVariants({ variant: "ghost", size: "icon" }),
          }}
        >
          <link.icon className="h-4 w-4" />
          <span className="sr-only">{title}</span>
        </Link>
      </TooltipTrigger>
      <TooltipContent side="right" className="flex items-center gap-4">
        {title}
        {label && (
          <span className="ml-auto text-muted-foreground">{label}</span>
        )}
      </TooltipContent>
    </Tooltip>
  ) : (
    <Link
      to={link.to}
      activeOptions={{ exact: true, includeSearch: false }}
      activeProps={{
        className: cn(
          buttonVariants({ variant: "default", size: "sm" }),
          "dark:bg-muted dark:text-white dark:hover:bg-muted dark:hover:text-white justify-start",
          "[&>span]:text-background [&>span]:dark:text-white",
        ),
      }}
      inactiveProps={{
        className: cn(
          buttonVariants({ variant: "ghost", size: "sm" }),
          "justify-start",
        ),
      }}
    >
      <link.icon className="mr-2 h-4 w-4" />
      {title}
      {label && <span className={cn("ml-auto")}>{label}</span>}
    </Link>
  );
}

function AvatarMenu() {
  const { data: me } = useQuery({
    ...userQueries.me(),
    staleTime: Infinity,
  });

  const avatarNumber = Math.floor(Math.random() * 649 + 1);
  const avatarUrl = `https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/${avatarNumber}.png`;

  const initials = me?.fullName
    ?.split(" ")
    .map((n) => n[0])
    .join("");

  return (
    <div>
      <Avatar className="size-10 bg-accent">
        <AvatarImage src={avatarUrl} />
        <AvatarFallback>{initials}</AvatarFallback>
      </Avatar>
    </div>
  );
}
