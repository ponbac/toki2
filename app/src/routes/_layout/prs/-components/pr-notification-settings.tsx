import { ListPullRequest } from "@/lib/api/queries/pullRequests";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuCheckboxItem,
} from "@/components/ui/dropdown-menu";
import { Bell } from "lucide-react";
import { Button } from "@/components/ui/button";
import { NotificationIcon } from "@/components/notification-icon";
import { NotificationType } from "@/lib/api/mutations/notifications";
import { useQuery } from "@tanstack/react-query";
import { notificationsQueries } from "@/lib/api/queries/notifications";
import { notificationsMutations } from "@/lib/api/mutations/notifications";
import { differsQueries } from "@/lib/api/queries/differs";
import { cn } from "@/lib/utils";
import * as React from "react";

export function PRNotificationSettings({
  pullRequest,
}: {
  pullRequest: ListPullRequest;
}) {
  const [open, setOpen] = React.useState(false);

  const { data: currentRepositoryId } = useQuery({
    ...differsQueries.differs(),
    staleTime: 1000 * 60 * 30, // 30 minutes
    select: (repositories) =>
      repositories.find(
        (r) =>
          r.organization === pullRequest.organization &&
          r.project === pullRequest.project &&
          r.repoName === pullRequest.repoName,
      )?.repoId,
  });

  const { data: preferences } = useQuery(
    notificationsQueries.preferences(currentRepositoryId!),
  );
  const { data: exceptions } = useQuery({
    ...notificationsQueries.prExceptions(currentRepositoryId!, pullRequest.id),
    enabled: !!currentRepositoryId,
  });

  const { mutate: setPrException, isPending: isSettingException } =
    notificationsMutations.useSetPrException();

  const handleToggle = (type: NotificationType, checked: boolean) => {
    setPrException({
      repositoryId: currentRepositoryId!,
      pullRequestId: pullRequest.id,
      exception: {
        repositoryId: currentRepositoryId!,
        pullRequestId: pullRequest.id,
        notificationType: type,
        enabled: checked,
      },
    });
  };

  const isEnabled = (type: NotificationType) => {
    const exceptionsEnabled = exceptions?.find(
      (e) => e.notificationType === type,
    )?.enabled;
    const preferencesEnabled = preferences?.find(
      (p) => p.notificationType === type,
    )?.enabled;

    return exceptionsEnabled ?? preferencesEnabled;
  };

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="icon" disabled={!currentRepositoryId}>
          <Bell className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="w-56"
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        <DropdownMenuLabel>Notify me when</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuCheckboxItem
          key={NotificationType.PrClosed}
          checked={isEnabled(NotificationType.PrClosed)}
          onSelect={(e) => {
            e.preventDefault();
          }}
          onCheckedChange={(checked) => {
            handleToggle(NotificationType.PrClosed, checked);
          }}
          className={cn(isEnabled(NotificationType.PrClosed) && "bg-primary")}
          disabled={isSettingException}
        >
          <span className="">Pull Request is closed</span>
          <NotificationIcon
            type={NotificationType.PrClosed}
            className="ml-2 size-4"
          />
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
