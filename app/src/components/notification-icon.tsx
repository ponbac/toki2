import { NotificationType } from "@/lib/api/mutations/notifications";
import {
  MessageSquarePlus,
  MessagesSquare,
  CheckSquare,
  AtSign,
} from "lucide-react";
import { match } from "ts-pattern";
import { cn } from "@/lib/utils";

export function NotificationIcon(props: {
  type: NotificationType;
  className?: string;
}) {
  return match(props.type)
    .with(NotificationType.ThreadAdded, () => (
      <MessageSquarePlus className={cn(props.className)} />
    ))
    .with(NotificationType.ThreadUpdated, () => (
      <MessagesSquare className={cn(props.className)} />
    ))
    .with(NotificationType.PrClosed, () => (
      <CheckSquare className={cn(props.className)} />
    ))
    .with(NotificationType.CommentMentioned, () => (
      <AtSign className={cn(props.className)} />
    ))
    .exhaustive();
}
