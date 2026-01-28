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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "./ui/dialog";
import { Input } from "./ui/input";
import { Button } from "./ui/button";
import { toast } from "sonner";
import { userMutations } from "@/lib/api/mutations/user";

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

  const initials = me?.fullName
    ?.split(" ")
    .map((n) => n[0])
    .join("");

  const [open, setOpen] = React.useState(false);
  const [selectedFile, setSelectedFile] = React.useState<File | null>(null);

  const uploadAvatar = userMutations.useUploadAvatar({
    onSuccess: () => {
      toast.success("Avatar updated");
      setSelectedFile(null);
      setOpen(false);
    },
    onError: () => {
      toast.error("Failed to upload avatar");
    },
  });

  const deleteAvatar = userMutations.useDeleteAvatar({
    onSuccess: () => {
      toast.success("Avatar removed");
    },
    onError: () => {
      toast.error("Failed to remove avatar");
    },
  });

  const handleFileChange: React.ChangeEventHandler<HTMLInputElement> = (e) => {
    const file = e.target.files?.[0] ?? null;
    if (!file) {
      setSelectedFile(null);
      return;
    }

    if (!file.type.startsWith("image/")) {
      toast.error("Please select an image file");
      return;
    }

    if (file.size > 5 * 1024 * 1024) {
      toast.error("Avatar must be smaller than 5MB");
      return;
    }

    setSelectedFile(file);
  };

  const avatarUrl = me?.avatarUrl ?? me?.picture ?? undefined;

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <button
          type="button"
          className="rounded-full focus:outline-none focus:ring-2 focus:ring-ring"
        >
          <Avatar className="size-10 bg-accent">
            {avatarUrl && <AvatarImage src={avatarUrl} />}
            <AvatarFallback>{initials}</AvatarFallback>
          </Avatar>
        </button>
      </DialogTrigger>

      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Account settings</DialogTitle>
          <DialogDescription>
            Upload a custom avatar image. This will replace your Azure avatar
            for you.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          <div className="flex items-center gap-4">
            <Avatar className="size-16 bg-accent">
              {avatarUrl && <AvatarImage src={avatarUrl} />}
              <AvatarFallback className="text-2xl">{initials}</AvatarFallback>
            </Avatar>
            <div className="text-sm text-muted-foreground">
              <p>Accepted formats: PNG, JPEG, WebP</p>
              <p>Max size: 5MB</p>
            </div>
          </div>

          <Input type="file" accept="image/*" onChange={handleFileChange} />
        </div>

        <DialogFooter>
          {me?.avatarUrl && (
            <Button
              variant="outline"
              onClick={() => deleteAvatar.mutate()}
              disabled={deleteAvatar.isPending}
            >
              Remove avatar
            </Button>
          )}
          <Button
            onClick={() =>
              selectedFile && uploadAvatar.mutate({ file: selectedFile })
            }
            disabled={!selectedFile || uploadAvatar.isPending}
          >
            Save avatar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
