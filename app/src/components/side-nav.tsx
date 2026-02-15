import * as React from "react";
import {
  LucideIcon,
  FolderGit2,
  GitPullRequest,
  Save,
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "./ui/dialog";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { Switch } from "./ui/switch";
import { toast } from "sonner";
import { useAtom } from "jotai/react";
import { userMutations } from "@/lib/api/mutations/user";
import { buildAvatarSources } from "@/lib/avatar";
import { enablePokemonAvatarFallbackAtom } from "@/lib/avatar-preferences";
import { useAvatarSourceWithFallback } from "@/hooks/useAvatarSourceWithFallback";

type LinkDestination = LinkProps<typeof router>["to"];
const MENU_ITEMS = [
  {
    title: "Pull requests",
    icon: GitPullRequest,
    variant: "ghost",
    to: "/prs",
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
  const [enablePokemonAvatarFallback, setEnablePokemonAvatarFallback] = useAtom(
    enablePokemonAvatarFallbackAtom,
  );

  const [open, setOpen] = React.useState(false);
  const [selectedFile, setSelectedFile] = React.useState<File | null>(null);
  const [selectedPreviewUrl, setSelectedPreviewUrl] = React.useState<string | null>(
    null,
  );
  const fileInputRef = React.useRef<HTMLInputElement>(null);

  const clearSelectedFile = React.useCallback(() => {
    setSelectedFile(null);
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  }, []);

  React.useEffect(() => {
    if (!selectedFile) {
      setSelectedPreviewUrl(null);
      return;
    }

    const objectUrl = URL.createObjectURL(selectedFile);
    setSelectedPreviewUrl(objectUrl);

    return () => {
      URL.revokeObjectURL(objectUrl);
    };
  }, [selectedFile]);

  const uploadAvatar = userMutations.useUploadAvatar({
    onSuccess: () => {
      toast.success("Avatar updated");
      clearSelectedFile();
      setOpen(false);
    },
    onError: () => {
      toast.error("Failed to upload avatar");
    },
  });

  const deleteAvatar = userMutations.useDeleteAvatar({
    onSuccess: () => {
      toast.success("Avatar removed");
      clearSelectedFile();
      setOpen(false);
    },
    onError: () => {
      toast.error("Failed to remove avatar");
    },
  });

  const avatarSources = React.useMemo(
    () => buildAvatarSources({
      preferredSources: [me?.avatarUrl, me?.picture],
      pokemonSeed: me?.email,
      enablePokemonFallback: enablePokemonAvatarFallback,
    }),
    [me?.avatarUrl, me?.picture, me?.email, enablePokemonAvatarFallback],
  );
  const { avatarSrc, onLoadingStatusChange: handleAvatarLoadingStatusChange } =
    useAvatarSourceWithFallback(avatarSources);

  const initials = me?.fullName
    ?.split(" ")
    .map((n) => n[0])
    .join("") ?? "?";

  const handleFileChange: React.ChangeEventHandler<HTMLInputElement> = (event) => {
    const file = event.target.files?.[0] ?? null;

    if (!file) {
      clearSelectedFile();
      return;
    }

    if (!file.type.startsWith("image/")) {
      clearSelectedFile();
      toast.error("Please choose an image file");
      return;
    }

    if (file.size > 5 * 1024 * 1024) {
      clearSelectedFile();
      toast.error("Avatar must be smaller than 5MB");
      return;
    }

    setSelectedFile(file);
  };

  const isMutating = uploadAvatar.isPending || deleteAvatar.isPending;

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen);
        if (!nextOpen) {
          clearSelectedFile();
        }
      }}
    >
      <DialogTrigger asChild>
        <button
          type="button"
          aria-label="Open avatar settings"
          className="group relative rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        >
          {/* Glow effect behind avatar */}
          <div className="absolute inset-0 rounded-full bg-primary/20 opacity-0 blur-md transition-opacity duration-300 group-hover:opacity-100" />
          <Avatar className="relative h-10 w-10 rounded-xl border border-border/50 bg-card shadow-sm transition-all duration-300 group-hover:border-primary/30 group-hover:shadow-glow-sm">
            <AvatarImage
              src={avatarSrc}
              className="rounded-xl"
              onLoadingStatusChange={handleAvatarLoadingStatusChange}
            />
            <AvatarFallback className="rounded-xl bg-gradient-to-br from-primary/20 to-primary/5 text-sm font-semibold">
              {initials}
            </AvatarFallback>
          </Avatar>
        </button>
      </DialogTrigger>

      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Avatar</DialogTitle>
          <DialogDescription>
            Upload a custom avatar to replace your default profile image.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          <div className="flex items-center gap-4">
            <Avatar className="size-16 bg-accent">
              <AvatarImage
                src={selectedPreviewUrl ?? avatarSrc}
                onLoadingStatusChange={
                  selectedPreviewUrl ? undefined : handleAvatarLoadingStatusChange
                }
              />
              <AvatarFallback className="text-2xl">{initials}</AvatarFallback>
            </Avatar>
            <div className="min-w-0 text-sm text-muted-foreground">
              <p>Accepted formats: PNG, JPEG, WebP</p>
              <p>Max size: 5MB</p>
              {selectedFile ? (
                <p className="mt-1 truncate text-xs text-foreground">
                  Previewing:{" "}
                  <span className="font-medium">{selectedFile.name}</span>
                </p>
              ) : (
                <p className="mt-1 text-xs">
                  Select an image to preview before saving.
                </p>
              )}
            </div>
          </div>

          <Input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            onChange={handleFileChange}
            disabled={isMutating}
          />
          {selectedFile && (
            <div className="flex items-center justify-between gap-2 rounded-md border border-border/60 bg-muted/30 px-3 py-2">
              <p className="min-w-0 truncate text-xs text-muted-foreground">
                Selected:{" "}
                <span className="font-medium text-foreground">
                  {selectedFile.name}
                </span>
              </p>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={clearSelectedFile}
                disabled={isMutating}
                className="h-7 px-2 text-xs"
              >
                Clear
              </Button>
            </div>
          )}

          <div className="rounded-md border border-border/60 bg-muted/30 px-3 py-2">
            <div className="flex items-start justify-between gap-3">
              <div className="space-y-0.5">
                <Label
                  htmlFor="pokemon-avatar-fallback"
                  className="text-sm font-medium text-foreground"
                >
                  Use Pokemon fallback avatars
                </Label>
                <p className="text-xs text-muted-foreground">
                  Show Pokemon avatars when profile images are unavailable.
                </p>
              </div>
              <Switch
                id="pokemon-avatar-fallback"
                checked={enablePokemonAvatarFallback}
                onCheckedChange={setEnablePokemonAvatarFallback}
              />
            </div>
          </div>
        </div>

        <DialogFooter>
          {me?.avatarUrl && (
            <Button
              variant="outline"
              onClick={() => deleteAvatar.mutate()}
              disabled={isMutating}
            >
              {deleteAvatar.isPending ? "Removing..." : "Remove avatar"}
            </Button>
          )}
          <Button
            type="button"
            onClick={() =>
              selectedFile && uploadAvatar.mutate({ file: selectedFile })
            }
            disabled={!selectedFile || isMutating}
            className="min-w-32 justify-center gap-2 text-center"
          >
            <Save className="size-4" />
            {uploadAvatar.isPending ? "Saving..." : "Save avatar"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
