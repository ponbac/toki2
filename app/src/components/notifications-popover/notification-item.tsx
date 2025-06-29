import { Differ } from "@/lib/api/queries/differs";
import { cn } from "@/lib/utils";
import { Check, CheckCircle2, ExternalLink } from "lucide-react";
import { Notification } from "@/lib/api/queries/notifications";
import { NotificationType } from "@/lib/api/mutations/notifications";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import { NotificationIcon } from "../notification-icon";
import { match } from "ts-pattern";

export function NotificationItem(props: {
  notification: Notification;
  repository: Differ | undefined;
  onView: () => void;
  isMarkingViewed: boolean;
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
        <div className="max-w-[26rem] flex-1">
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
              disabled={props.isMarkingViewed}
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
    [NotificationType.CommentMentioned]: "text-purple-500",
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
          .with(NotificationType.CommentMentioned, () => "You were mentioned")
          .exhaustive()}
      </TooltipContent>
    </Tooltip>
  );
}
