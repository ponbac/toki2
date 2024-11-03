import { Bell } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { notificationsQueries, Notification } from "@/lib/api/queries/notifications";
import { ScrollArea } from "@/components/ui/scroll-area";
import { notificationsMutations } from "@/lib/api/mutations/notifications";

export function NotificationsMenu() {
  const { data: notifications = [] } = useQuery({
    ...notificationsQueries.notifications({ includeViewed: false }),
  });

  const { mutate: markViewed } = notificationsMutations.useMarkNotificationViewed();

  const unviewedCount = notifications.filter(n => !n.viewedAt).length;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          className={cn(
            buttonVariants({ variant: "ghost", size: "icon" }),
            "relative h-9 w-9"
          )}
        >
          <Bell className="h-4 w-4" />
          {unviewedCount > 0 && (
            <span className="absolute right-1 top-1 flex h-4 w-4 items-center justify-center rounded-full bg-primary text-[10px] text-primary-foreground">
              {unviewedCount}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-80 p-0" align="start" side="right">
        <div className="p-2 font-medium">Notifications</div>
        <ScrollArea className="h-[400px]">
          {notifications.length === 0 ? (
            <div className="p-4 text-sm text-muted-foreground">
              No notifications
            </div>
          ) : (
            <div className="grid gap-1">
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
  onView
}: { 
  notification: Notification;
  onView: () => void;
}) {
  return (
    <a
      href={notification.link}
      className={cn(
        "flex flex-col gap-1 px-4 py-2 hover:bg-muted",
        !notification.viewedAt && "bg-muted/50"
      )}
      onClick={() => {
        if (!notification.viewedAt) {
          onView();
        }
      }}
    >
      <div className="font-medium">{notification.title}</div>
      <div className="text-sm text-muted-foreground">{notification.message}</div>
      <div className="text-xs text-muted-foreground">
        {new Date(notification.createdAt).toLocaleDateString()}
      </div>
    </a>
  );
} 