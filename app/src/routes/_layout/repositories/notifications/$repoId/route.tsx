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
import {
  NotificationRule,
  notificationsQueries,
} from "@/lib/api/queries/notifications";
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

  function handleToggle(
    type: NotificationType,
    enabled: boolean,
    isPush: boolean = false,
  ) {
    const preference = preferences?.find((p) => p.notificationType === type);
    updatePreference({
      repositoryId: Number(repoId),
      rule: {
        id: preference?.id ?? 0,
        userId: me?.id ?? 0,
        repositoryId: Number(repoId),
        notificationType: type,
        enabled: isPush ? (preference?.enabled ?? false) : enabled,
        pushEnabled: isPush ? enabled : (preference?.pushEnabled ?? false),
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
            You can enable site notifications, push notifications, or both.
          </DialogDescription>
        </DialogHeader>

        {isLoading ? (
          <div className="flex justify-center p-4">
            <LoadingSpinner />
          </div>
        ) : (
          <div className="space-y-4 pt-2">
            <div className="grid grid-cols-[1fr,auto,auto] items-center gap-4 border-b pb-2">
              <div /> {/* Empty space for alignment */}
              <Label className="px-2 text-sm font-medium text-muted-foreground">
                Site
              </Label>
              <Label className="px-2 text-sm font-medium text-muted-foreground">
                Push
              </Label>
            </div>

            <div className="space-y-4">
              <NotificationRow
                type={NotificationType.PrClosed}
                title="Pull Request Closed"
                description="Get notified when a pull request is completed or abandoned."
                preferences={preferences}
                onToggle={handleToggle}
              />

              <NotificationRow
                type={NotificationType.ThreadAdded}
                title="Thread Added"
                description="Get notified when someone starts a new review thread in one of your pull requests."
                preferences={preferences}
                onToggle={handleToggle}
              />

              <NotificationRow
                type={NotificationType.ThreadUpdated}
                title="Thread Updated"
                description="Get notified when someone replies to a review thread you are part of."
                preferences={preferences}
                onToggle={handleToggle}
              />
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function NotificationRow(props: {
  type: NotificationType;
  title: string;
  description: string;
  preferences?: Array<NotificationRule>;
  onToggle: (
    type: NotificationType,
    enabled: boolean,
    isPush?: boolean,
  ) => void;
}) {
  const preference = props.preferences?.find(
    (p) => p.notificationType === props.type,
  );

  return (
    <div className="grid grid-cols-[1fr,auto,auto] items-center gap-4">
      <div className="flex gap-3">
        <NotificationIcon
          type={props.type}
          className="mt-0.5 h-5 w-5 shrink-0 text-muted-foreground"
        />
        <div>
          <Label>{props.title}</Label>
          <p className="text-sm text-muted-foreground">{props.description}</p>
        </div>
      </div>
      <Switch
        id={`${props.type}-site`}
        checked={preference?.enabled ?? false}
        onCheckedChange={(checked) => props.onToggle(props.type, checked)}
      />
      <Switch
        id={`${props.type}-push`}
        checked={preference?.pushEnabled ?? false}
        onCheckedChange={(checked) => props.onToggle(props.type, checked, true)}
      />
    </div>
  );
}
