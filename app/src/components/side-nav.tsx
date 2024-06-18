import * as React from "react";
import {
  LucideIcon,
  FolderGit2,
  Activity,
  GitPullRequest,
  AlarmClock,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "./ui/resizable";
import { Separator } from "./ui/separator";
import { buttonVariants } from "./ui/button";
import { Link, LinkProps } from "@tanstack/react-router";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./ui/select";
import { ScrollArea } from "./ui/scroll-area";
import { ThemeToggle } from "./theme-toggle";
import { router } from "@/main";

type LinkDestination = LinkProps<typeof router>["to"];
const MENU_ITEMS = [
  {
    title: "Pull requests",
    label: "",
    icon: GitPullRequest,
    variant: "ghost",
    to: "/prs",
  },
  {
    title: "Commits",
    label: "",
    icon: Activity,
    variant: "ghost",
    to: "/prs/commits",
  },
  {
    title: "Repositories",
    label: "",
    icon: FolderGit2,
    variant: "ghost",
    to: "/repositories",
  },
  {
    title: "Milltime",
    label: "",
    icon: AlarmClock,
    variant: "ghost",
    to: "/milltime",
  },
] as const satisfies readonly {
  title: string;
  label?: string;
  icon: LucideIcon;
  variant: "default" | "ghost";
  to: LinkDestination;
}[];

type UsedLink = (typeof MENU_ITEMS)[number]["to"];

export function SideNavWrapper({
  accounts,
  defaultLayout = [3, 97],
  defaultCollapsed = false,
  navCollapsedSize,
  children,
  className,
}: {
  accounts: {
    label: string;
    email: string;
    icon: React.ReactNode;
  }[];
  defaultLayout?: number[];
  defaultCollapsed?: boolean;
  navCollapsedSize: number;
  children: React.ReactNode;
  className?: string;
}) {
  const [isCollapsed, setIsCollapsed] = React.useState(defaultCollapsed);

  return (
    <ResizablePanelGroup
      direction="horizontal"
      onLayout={(sizes: number[]) => {
        document.cookie = `react-resizable-panels:layout=${JSON.stringify(
          sizes,
        )}`;
      }}
      className="sticky top-0 h-full max-h-screen min-h-screen items-stretch"
    >
      <ResizablePanel
        defaultSize={defaultLayout[0]}
        collapsedSize={navCollapsedSize}
        collapsible={true}
        minSize={8}
        maxSize={12}
        onCollapse={() => {
          setIsCollapsed(true);
          document.cookie = `react-resizable-panels:collapsed=${JSON.stringify(
            true,
          )}`;
        }}
        onExpand={() => {
          setIsCollapsed(false);
          document.cookie = `react-resizable-panels:collapsed=${JSON.stringify(
            false,
          )}`;
        }}
        className={cn(
          isCollapsed && "min-w-[50px] transition-all duration-300 ease-in-out",
        )}
      >
        <div
          className={cn(
            "flex h-[52px] items-center justify-center",
            isCollapsed ? "h-[52px]" : "px-2",
          )}
        >
          <AccountSwitcher isCollapsed={isCollapsed} accounts={accounts} />
        </div>
        <Separator />
        <Nav isCollapsed={isCollapsed} links={MENU_ITEMS} />
      </ResizablePanel>
      <ResizableHandle withHandle />
      <ResizablePanel
        defaultSize={defaultLayout[1]}
        minSize={30}
        className={className}
      >
        <ScrollArea className="h-screen">{children}</ScrollArea>
      </ResizablePanel>
    </ResizablePanelGroup>
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
        <ThemeToggle />
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

export function AccountSwitcher({
  isCollapsed,
  accounts,
}: {
  isCollapsed: boolean;
  accounts: {
    label: string;
    email: string;
    icon: React.ReactNode;
  }[];
}) {
  const [selectedAccount, setSelectedAccount] = React.useState<string>(
    accounts[0].email,
  );

  return (
    <Select defaultValue={selectedAccount} onValueChange={setSelectedAccount}>
      <SelectTrigger
        className={cn(
          "flex items-center gap-2 [&>span]:line-clamp-1 [&>span]:flex [&>span]:w-full [&>span]:items-center [&>span]:gap-1 [&>span]:truncate [&_svg]:h-4 [&_svg]:w-4 [&_svg]:shrink-0",
          isCollapsed &&
            "flex h-9 w-9 shrink-0 items-center justify-center p-0 [&>span]:w-auto [&>svg]:hidden",
        )}
        aria-label="Select account"
      >
        <SelectValue placeholder="Select an account">
          {accounts.find((account) => account.email === selectedAccount)?.icon}
          <span className={cn("ml-2", isCollapsed && "hidden")}>
            {
              accounts.find((account) => account.email === selectedAccount)
                ?.label
            }
          </span>
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {accounts.map((account) => (
          <SelectItem key={account.email} value={account.email}>
            <div className="flex items-center gap-3 [&_svg]:h-4 [&_svg]:w-4 [&_svg]:shrink-0 [&_svg]:text-foreground">
              {account.icon}
              {account.email}
            </div>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
