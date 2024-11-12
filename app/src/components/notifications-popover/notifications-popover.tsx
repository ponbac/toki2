import { Bell } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { notificationsQueries } from "@/lib/api/queries/notifications";
import { ScrollArea } from "@/components/ui/scroll-area";
import { notificationsMutations } from "@/lib/api/mutations/notifications";
import { differsQueries } from "@/lib/api/queries/differs";
import { useEffect } from "react";
import { useTitleStore } from "@/hooks/useTitleStore";
import { NotificationItem } from "./notification-item";
import { NotificationSettingsDropdown } from "./notification-settings-dropdown";

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
          <div className="flex flex-row items-center gap-1">
            {hasUnviewedNotifications && (
              <div className="text-xs text-muted-foreground">
                {unviewedCount} unread
              </div>
            )}
            <NotificationSettingsDropdown />
          </div>
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

