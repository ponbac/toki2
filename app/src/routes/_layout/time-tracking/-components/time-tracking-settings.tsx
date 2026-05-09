import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { timeTrackingMutations } from "@/lib/api/mutations/time-tracking";
import { apiErrorToast } from "@/lib/api/errors";
import { timeTrackingQueries } from "@/lib/api/queries/time-tracking";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon, Settings2Icon, UserCheckIcon } from "lucide-react";
import { toast } from "sonner";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export const TimeTrackingSettings = ({
  rememberLastProject,
  setRememberLastProject,
  isAdmin,
}: {
  rememberLastProject: boolean;
  setRememberLastProject: (value: boolean) => void;
  isAdmin: boolean;
}) => {
  const { data: adminMappings } = useQuery({
    ...timeTrackingQueries.adminMappings(),
    enabled: isAdmin,
  });
  const { mutate: importKleerUsers, isPending: isImporting } =
    timeTrackingMutations.useImportKleerUsers({
      onSuccess: () => toast.success("Kleer users synced"),
      onError: apiErrorToast("Failed to sync Kleer users"),
    });
  const { mutate: linkKleerUsersByEmail, isPending: isLinkingByEmail } =
    timeTrackingMutations.useLinkKleerUsersByEmail({
      onSuccess: (data) => {
        const count = data.createdLinkCount;
        if (count === 0) {
          toast.info("No matching emails found");
          return;
        }

        toast.success(
          count === 1 ? "1 mapping created" : `${count} mappings created`,
        );
      },
      onError: apiErrorToast("Failed to match Kleer users"),
    });
  const { mutate: upsertLink, isPending: isSavingLink } =
    timeTrackingMutations.useUpsertKleerUserLink({
      onSuccess: () => toast.success("Mapping saved"),
      onError: apiErrorToast("Failed to save mapping"),
    });
  const { mutate: deactivateLink, isPending: isRemovingLink } =
    timeTrackingMutations.useDeactivateKleerUserLink({
      onSuccess: () => toast.success("Mapping removed"),
      onError: apiErrorToast("Failed to remove mapping"),
    });
  const isMappingBusy = isSavingLink || isRemovingLink || isLinkingByEmail;

  return (
    <Popover>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 text-muted-foreground hover:bg-muted/60 hover:text-foreground"
              >
                <Settings2Icon className="h-4 w-4" />
                <span className="sr-only">Settings</span>
              </Button>
            </PopoverTrigger>
          </TooltipTrigger>
          <TooltipContent>
            <p>Settings</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <PopoverContent
        align="end"
        className="w-[26rem] max-w-[calc(100vw-1rem)] overflow-y-auto overflow-x-hidden rounded-xl border-border/70 bg-card/95 p-4 text-card-foreground shadow-elevated backdrop-blur supports-[backdrop-filter]:bg-card/90"
        style={{
          maxHeight:
            "min(calc(100vh - 2rem), var(--radix-popover-content-available-height))",
        }}
      >
        <div className="flex max-h-[calc(100vh-4rem)] flex-col gap-4">
          <section className="shrink-0 space-y-3">
            <h4 className="text-sm font-semibold leading-none text-foreground">
              Preferences
            </h4>
            <div className="flex items-center justify-between rounded-lg border border-border/60 bg-background/60 px-3 py-2">
              <div className="space-y-0.5">
                <Label
                  htmlFor="remember-project"
                  className="text-sm font-medium text-foreground"
                >
                  Remember project
                </Label>
                <p className="pr-3 text-xs leading-relaxed text-muted-foreground">
                  Auto-fill last used project and activity for new timers.
                </p>
              </div>
              <Switch
                id="remember-project"
                checked={rememberLastProject}
                onCheckedChange={setRememberLastProject}
              />
            </div>
          </section>

          {isAdmin && (
            <section className="flex min-h-0 flex-1 flex-col gap-3 border-t border-border/60 pt-4">
              <div className="flex shrink-0 items-center justify-between gap-3">
                <div>
                  <h4 className="text-sm font-semibold leading-none text-foreground">
                    Kleer mappings
                  </h4>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Assign Toki users to imported Kleer users.
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5"
                    disabled={isImporting || isMappingBusy}
                    onClick={() => importKleerUsers()}
                  >
                    <RefreshCwIcon
                      className={
                        isImporting ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"
                      }
                    />
                    Sync
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-8 gap-1.5"
                    disabled={
                      isImporting ||
                      isMappingBusy ||
                      !adminMappings?.kleerUsers.length
                    }
                    onClick={() => linkKleerUsersByEmail()}
                  >
                    <UserCheckIcon
                      className={
                        isLinkingByEmail
                          ? "h-3.5 w-3.5 animate-pulse"
                          : "h-3.5 w-3.5"
                      }
                    />
                    Match
                  </Button>
                </div>
              </div>

              <ScrollArea className="h-[clamp(6rem,calc(100vh-17rem),22rem)] pr-3">
                <div className="space-y-2">
                  {adminMappings?.kleerUsers.length ? (
                    adminMappings.kleerUsers.map((kleerUser) => (
                      <div
                        key={kleerUser.providerUserId}
                        className="rounded-lg border border-border/60 bg-background/60 p-3"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <p className="truncate text-sm font-medium text-foreground">
                              {kleerUser.name}
                            </p>
                            <p className="truncate text-xs text-muted-foreground">
                              {kleerUser.email ?? kleerUser.providerUserId}
                            </p>
                          </div>
                          <span
                            className={
                              kleerUser.active
                                ? "text-xs text-emerald-600"
                                : "text-xs text-muted-foreground"
                            }
                          >
                            {kleerUser.active ? "Active" : "Inactive"}
                          </span>
                        </div>
                        <Select
                          value={
                            kleerUser.mappedUserId?.toString() ?? "unmapped"
                          }
                          disabled={!kleerUser.active || isMappingBusy}
                          onValueChange={(value) => {
                            if (value === "unmapped") {
                              if (kleerUser.mappedUserId) {
                                deactivateLink({
                                  userId: kleerUser.mappedUserId,
                                });
                              }
                              return;
                            }

                            upsertLink({
                              userId: Number(value),
                              providerUserId: kleerUser.providerUserId,
                            });
                          }}
                        >
                          <SelectTrigger className="mt-3 h-8">
                            <SelectValue placeholder="Unmapped" />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="unmapped">Unmapped</SelectItem>
                            {adminMappings.users.map((user) => (
                              <SelectItem key={user.id} value={String(user.id)}>
                                {user.fullName} ({user.email})
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      </div>
                    ))
                  ) : (
                    <div className="rounded-lg border border-dashed border-border/70 px-3 py-6 text-center text-sm text-muted-foreground">
                      Sync Kleer users to start mapping accounts.
                    </div>
                  )}
                </div>
              </ScrollArea>
            </section>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
};
