import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { useAtom } from "jotai/react";
import { Save } from "lucide-react";
import { toast } from "sonner";
import { ApiTokensSettings } from "./api-tokens-settings";
import { Avatar, AvatarFallback, AvatarImage } from "./ui/avatar";
import { Button } from "./ui/button";
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
import { Label } from "./ui/label";
import { Switch } from "./ui/switch";
import { useAvatarSourceWithFallback } from "@/hooks/useAvatarSourceWithFallback";
import { useApiTokenIssuance } from "@/hooks/use-api-token-issuance";
import { userMutations } from "@/lib/api/mutations/user";
import { userQueries } from "@/lib/api/queries/user";
import { buildAvatarSources } from "@/lib/avatar";
import { enablePokemonAvatarFallbackAtom } from "@/lib/avatar-preferences";

/** Navigation avatar trigger and cohesive account-settings dialog. */
export function AccountSettings() {
  const { data: me } = useQuery({
    ...userQueries.me(),
    staleTime: Infinity,
  });
  const [enablePokemonAvatarFallback, setEnablePokemonAvatarFallback] = useAtom(
    enablePokemonAvatarFallbackAtom,
  );
  const tokenIssuance = useApiTokenIssuance();

  const [open, setOpen] = React.useState(false);
  const [selectedFile, setSelectedFile] = React.useState<File | null>(null);
  const [selectedPreviewUrl, setSelectedPreviewUrl] = React.useState<
    string | null
  >(null);
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
    () =>
      buildAvatarSources({
        preferredSources: [me?.avatarUrl, me?.picture],
        pokemonSeed: me?.email,
        enablePokemonFallback: enablePokemonAvatarFallback,
      }),
    [me?.avatarUrl, me?.picture, me?.email, enablePokemonAvatarFallback],
  );
  const { avatarSrc, onLoadingStatusChange: handleAvatarLoadingStatusChange } =
    useAvatarSourceWithFallback(avatarSources);

  const initials =
    me?.fullName
      ?.split(" ")
      .map((name) => name[0])
      .join("") ?? "?";

  const handleFileChange: React.ChangeEventHandler<HTMLInputElement> = (
    event,
  ) => {
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

  const isMutating =
    uploadAvatar.isPending || deleteAvatar.isPending || tokenIssuance.isPending;

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && tokenIssuance.isPending) {
          toast.info(
            "Wait for token creation to finish before closing settings",
          );
          return;
        }
        setOpen(nextOpen);
        if (!nextOpen) {
          clearSelectedFile();
        }
      }}
    >
      <DialogTrigger asChild>
        <button
          type="button"
          aria-label="Open account settings"
          className="group relative rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
        >
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

      <DialogContent className="max-h-[85vh] max-w-md grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden">
        <DialogHeader>
          <DialogTitle>Account</DialogTitle>
          <DialogDescription>
            Avatar, fallback images, and API tokens.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-col gap-4 overflow-y-auto py-2 pr-1">
          <div className="flex items-center gap-4">
            <Avatar className="size-16 bg-accent">
              <AvatarImage
                src={selectedPreviewUrl ?? avatarSrc}
                onLoadingStatusChange={
                  selectedPreviewUrl
                    ? undefined
                    : handleAvatarLoadingStatusChange
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
                disabled={isMutating}
              />
            </div>
          </div>

          <ApiTokensSettings issuance={tokenIssuance} />
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
