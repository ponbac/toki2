import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { notificationsQueries } from "@/lib/api/queries/notifications";
import {
  notificationsMutations,
  NotificationType,
} from "@/lib/api/mutations/notifications";
import { LoadingSpinner } from "@/components/loading-spinner";
import { toast } from "sonner";
import { useQuery } from "@tanstack/react-query";
import { userQueries } from "@/lib/api/queries/user";
import { NotificationIcon } from "@/components/notification-icon";

export const Route = createFileRoute(
  "/_layout/repositories/notifications/$repoId",
)({
  component: NotificationsDialog,
});

function NotificationsDialog() {
  const { repoId } = Route.useParams();
  const navigate = useNavigate({ from: Route.fullPath });

  const { data: me } = useQuery({
    ...userQueries.me(),
  });

  const { data: preferences, isLoading } = useQuery({
    ...notificationsQueries.preferences(Number(repoId)),
    enabled: !!repoId,
  });

  const { mutate: updatePreference } =
    notificationsMutations.useUpdatePreferences({
      onError: () => {
        toast.error("Failed to update notification preferences");
      },
    });

  function handleToggle(type: NotificationType, enabled: boolean) {
    updatePreference({
      repositoryId: Number(repoId),
      rule: {
        id: preferences?.find((p) => p.notificationType === type)?.id ?? 0,
        userId: me?.id ?? 0,
        repositoryId: Number(repoId),
        notificationType: type,
        enabled,
      },
    });
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          navigate({ to: "/repositories" });
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Notifications</DialogTitle>
          <DialogDescription className="text-balance text-sm">
            Choose which notifications you want to receive for this repository.
            These settings can be overridden for individual pull requests.
          </DialogDescription>
        </DialogHeader>

        {isLoading ? (
          <div className="flex justify-center p-4">
            <LoadingSpinner />
          </div>
        ) : (
          <div className="space-y-6">
            <div className="space-y-2">
              <div className="flex items-center justify-between gap-4">
                <div className="flex gap-3">
                  <NotificationIcon
                    type={NotificationType.PrClosed}
                    className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground"
                  />
                  <div>
                    <Label htmlFor="pr-closed">Pull Request Closed</Label>
                    <p className="text-sm text-muted-foreground">
                      Get notified when a pull request is completed or abandoned
                    </p>
                  </div>
                </div>
                <Switch
                  id="pr-closed"
                  checked={
                    preferences?.find(
                      (p) => p.notificationType === NotificationType.PrClosed,
                    )?.enabled ?? false
                  }
                  onCheckedChange={(checked) =>
                    handleToggle(NotificationType.PrClosed, checked)
                  }
                />
              </div>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between gap-4">
                <div className="flex gap-3">
                  <NotificationIcon
                    type={NotificationType.ThreadAdded}
                    className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground"
                  />
                  <div>
                    <Label htmlFor="thread-added">Thread Added</Label>
                    <p className="text-sm text-muted-foreground">
                      Get notified when someone starts a new review thread
                    </p>
                  </div>
                </div>
                <Switch
                  id="thread-added"
                  checked={
                    preferences?.find(
                      (p) =>
                        p.notificationType === NotificationType.ThreadAdded,
                    )?.enabled ?? false
                  }
                  onCheckedChange={(checked) =>
                    handleToggle(NotificationType.ThreadAdded, checked)
                  }
                />
              </div>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between gap-4">
                <div className="flex gap-3">
                  <NotificationIcon
                    type={NotificationType.ThreadUpdated}
                    className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground"
                  />
                  <div>
                    <Label htmlFor="thread-updated">Thread Updated</Label>
                    <p className="text-sm text-muted-foreground">
                      Get notified when someone replies to a review thread
                    </p>
                  </div>
                </div>
                <Switch
                  id="thread-updated"
                  checked={
                    preferences?.find(
                      (p) =>
                        p.notificationType === NotificationType.ThreadUpdated,
                    )?.enabled ?? false
                  }
                  onCheckedChange={(checked) =>
                    handleToggle(NotificationType.ThreadUpdated, checked)
                  }
                />
              </div>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
