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
import { NotificationsPopover } from "./notifications-popover/notifications-popover";

type LinkDestination = LinkProps<typeof router>["to"];
const MENU_ITEMS = [
  {
    title: "Milltime",
    icon: TimerIcon,
    variant: "ghost",
    to: "/milltime",
  },
  {
    title: "Pull requests",
    icon: GitPullRequest,
    variant: "ghost",
    to: "/prs",
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
      <div className="fixed left-0 right-0 top-0 z-50 flex h-14 flex-row border-b bg-background md:h-full md:w-14 md:flex-col md:items-stretch md:border-b-0 md:border-r">
        <div className="flex w-full flex-row justify-center space-x-2 px-4 md:h-auto md:flex-col md:space-x-0 md:space-y-2 md:px-0 md:py-2">
          <div className="flex h-full items-center justify-center md:h-[52px]">
            <AvatarMenu />
          </div>
          <div className="flex h-full items-center justify-center md:h-[52px]">
            <NotificationsPopover />
          </div>
          <div className="flex h-full items-center justify-center md:hidden">
            <Separator orientation="vertical" className="h-6" />
          </div>
          <Nav links={MENU_ITEMS} />
        </div>
      </div>
      <div className="mt-14 flex-1 md:ml-16 md:mt-0">
        <ScrollArea className="h-[calc(100vh-3.5rem)] md:h-screen">
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
    <div className="flex h-full items-center md:h-auto md:flex-col md:gap-4">
      <nav className="flex flex-row items-center space-x-2 md:flex-col md:space-x-0 md:space-y-2">
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
          className={cn("size-9")}
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
          <link.icon className="scale-125" />
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
  );
}

function AvatarMenu() {
  const { data: me } = useQuery({
    ...userQueries.me(),
    staleTime: Infinity,
  });

  const avatarNumber = React.useMemo(
    () => Math.floor(Math.random() * 649 + 1),
    [],
  );
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
