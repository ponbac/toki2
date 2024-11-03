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
import { notificationsMutations } from "@/lib/api/mutations/notifications";

export function NotificationsMenu() {
  const { data: notifications = [] } = useQuery({
    ...notificationsQueries.notifications({ includeViewed: true }),
    refetchInterval: 1000 * 30,
  });

  const { mutate: markViewed } =
    notificationsMutations.useMarkNotificationViewed();

  const unviewedCount = notifications.filter((n) => !n.viewedAt).length;
  const hasUnviewedNotifications = unviewedCount > 0;

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
      <PopoverContent className="w-96 p-0" align="end" side="bottom">
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

function NotificationItem({
  notification,
  onView,
}: {
  notification: Notification;
  onView: () => void;
}) {
  return (
    <div
      className={cn(
        "group flex flex-col gap-1 border-b px-4 py-3 transition-colors",
        notification.viewedAt ? "bg-background opacity-75" : "bg-muted/30",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1">
          <div className="mb-0.5 font-medium">{notification.title}</div>
          <div className="text-sm text-muted-foreground">
            {notification.message}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {new Date(notification.createdAt).toLocaleString("sv-SE", {
              year: "numeric",
              month: "2-digit",
              day: "2-digit",
              hour: "2-digit",
              minute: "2-digit",
            })}
          </div>
        </div>
        <div className="mt-1 flex items-center gap-1 self-start">
          {notification.viewedAt ? (
            <CheckCircle2 className="h-4 w-4 text-muted-foreground/50" />
          ) : (
            <button
              onClick={onView}
              className="rounded-md p-1 opacity-0 transition-opacity hover:bg-muted group-hover:opacity-100"
              title="Mark as read"
            >
              <Check className="h-4 w-4" />
            </button>
          )}
          {!!notification.link && (
            <a
              href={notification.link}
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
    </div>
  );
}
