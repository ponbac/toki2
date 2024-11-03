import { Bell, ExternalLink, Check, CheckCircle2 } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  notificationsQueries,
  Notification,
} from "@/lib/api/queries/notifications";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  notificationsMutations,
  NotificationType,
} from "@/lib/api/mutations/notifications";
import { match } from "ts-pattern";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";
import { NotificationIcon } from "./notification-icon";
import { Differ, differsQueries } from "@/lib/api/queries/differs";
import { useEffect } from "react";
import { useTitleStore } from "@/hooks/useTitleStore";

export function NotificationsPopover() {
  const { addSegment, removeSegment } = useTitleStore();

  const { data: notifications = [] } = useQuery({
    ...notificationsQueries.notifications({ includeViewed: true }),
    refetchInterval: 1000 * 30, // 30 seconds
  });
  const { data: repositories } = useQuery({
    ...differsQueries.differs(),
    staleTime: 1000 * 60 * 30, // 30 minutes
  });

  const { mutate: markViewed } =
    notificationsMutations.useMarkNotificationViewed();

  const unviewedCount = notifications.filter((n) => !n.viewedAt).length;
  const hasUnviewedNotifications = unviewedCount > 0;

  // useEffect that prepends the number of unviewed notifications to the document title
  useEffect(() => {
    if (unviewedCount > 0) {
      addSegment({
        id: "notifications",
        title: `(${unviewedCount})`,
      });
    } else {
      removeSegment("notifications");
    }
  }, [unviewedCount, addSegment, removeSegment]);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          className={cn(
            buttonVariants({ variant: "ghost", size: "icon" }),
            "relative h-9 w-9 transition-colors hover:bg-muted/80",
          )}
        >
          <Bell className="h-4 w-4" />
          {hasUnviewedNotifications && (
            <span className="absolute right-1 top-1 flex h-4 w-4 items-center justify-center rounded-full bg-primary text-[10px] text-primary-foreground duration-200 animate-in zoom-in-50">
              {unviewedCount}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-[32rem] p-0" align="start" side="right">
        <div className="flex items-center justify-between border-b p-3">
          <div className="font-semibold">Notifications</div>
          {hasUnviewedNotifications && (
            <div className="text-xs text-muted-foreground">
              {unviewedCount} unread
            </div>
          )}
        </div>
        <ScrollArea className="h-[400px]">
          {notifications.length === 0 ? (
            <div className="p-8 text-center text-sm text-muted-foreground">
              <Bell className="mx-auto mb-2 h-8 w-8 opacity-50" />
              No notifications
            </div>
          ) : (
            <div className="grid">
              {notifications.map((notification) => (
                <NotificationItem
                  key={notification.id}
                  notification={notification}
                  repository={repositories?.find(
                    (r) => r.repoId === notification.repositoryId,
                  )}
                  onView={() => markViewed(notification.id)}
                />
              ))}
            </div>
          )}
        </ScrollArea>
      </PopoverContent>
    </Popover>
  );
}

function NotificationItem(props: {
  notification: Notification;
  repository: Differ | undefined;
  onView: () => void;
}) {
  return (
    <div
      className={cn(
        "group flex flex-col gap-1 border-b px-4 py-3 transition-colors",
        props.notification.viewedAt
          ? "bg-background opacity-75"
          : "bg-muted/30",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1">
          <div className="mb-0.5 flex items-center gap-2">
            <ColoredNotificationIconWithTooltip
              type={props.notification.notificationType}
            />
            <span className="truncate font-medium">
              {props.notification.title}
            </span>
          </div>
          <div className="text-sm text-muted-foreground">
            {props.notification.message}
          </div>
        </div>
        <div className="flex items-center gap-1 self-start">
          {props.notification.viewedAt ? (
            <CheckCircle2 className="h-4 w-4 text-muted-foreground/50" />
          ) : (
            <button
              onClick={props.onView}
              className="rounded-md p-1 opacity-0 transition-opacity hover:bg-muted group-hover:opacity-100"
              title="Mark as read"
            >
              <Check className="h-4 w-4" />
            </button>
          )}
          {!!props.notification.link && (
            <a
              href={props.notification.link}
              className="rounded-md p-1 opacity-0 transition-opacity hover:bg-muted group-hover:opacity-100"
              target="_blank"
              rel="noopener noreferrer"
              title="Open link"
            >
              <ExternalLink className="h-4 w-4" />
            </a>
          )}
        </div>
      </div>
      <div className="flex items-center justify-between pt-1 text-xs text-muted-foreground">
        <div>
          {new Date(props.notification.createdAt).toLocaleString("sv-SE", {
            year: "numeric",
            month: "2-digit",
            day: "2-digit",
            hour: "2-digit",
            minute: "2-digit",
          })}
        </div>
        {!!props.repository && (
          <div className="flex items-center gap-0.5 font-mono">
            <span>{props.repository.organization}</span>
            <span>/</span>
            <span>{props.repository.project}</span>
            <span>/</span>
            <span>{props.repository.repoName}</span>
          </div>
        )}
      </div>
    </div>
  );
}

function ColoredNotificationIconWithTooltip(props: { type: NotificationType }) {
  const iconColorClasses = {
    [NotificationType.ThreadAdded]: "text-blue-500",
    [NotificationType.ThreadUpdated]: "text-yellow-500",
    [NotificationType.PrClosed]: "text-red-500",
  };

  return (
    <Tooltip>
      <TooltipTrigger className="cursor-default">
        <NotificationIcon
          type={props.type}
          className={cn("h-4 w-4", iconColorClasses[props.type])}
        />
      </TooltipTrigger>
      <TooltipContent>
        {match(props.type)
          .with(NotificationType.ThreadAdded, () => "New thread added")
          .with(NotificationType.ThreadUpdated, () => "Thread updated")
          .with(NotificationType.PrClosed, () => "Pull request closed")
          .exhaustive()}
      </TooltipContent>
    </Tooltip>
  );
}
