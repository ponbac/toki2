import { queryOptions } from "@tanstack/react-query";
import { api } from "../api";
import { NotificationType } from "../mutations/notifications";

export const notificationsQueries = {
  notifications: (options: { includeViewed: boolean }) =>
    queryOptions({
      queryKey: ["notifications", "list", options],
      queryFn: async () =>
        api
          .get("notifications", {
            searchParams: { includeViewed: options.includeViewed },
          })
          .json<Array<Notification>>(),
    }),
  preferences: (repositoryId: number) =>
    queryOptions({
      queryKey: ["notifications", "preferences", repositoryId],
      queryFn: async () =>
        api
          .get(`notifications/preferences/${repositoryId}`)
          .json<Array<NotificationRule>>(),
    }),
  prExceptions: (repositoryId: number, pullRequestId: number) =>
    queryOptions({
      queryKey: ["notifications", "exceptions", repositoryId, pullRequestId],
      queryFn: async () =>
        api
          .get(
            `notifications/repositories/${repositoryId}/pull-requests/${pullRequestId}/exceptions`,
          )
          .json<Array<PrNotificationException>>(),
    }),
  isSubscribed: () =>
    queryOptions({
      queryKey: ["notifications", "is-subscribed"],
      queryFn: async () =>
        api.get("notifications/is-subscribed").json<boolean>(),
    }),
};

export type Notification = {
  id: number;
  userId: number;
  repositoryId: number;
  pullRequestId: number;
  notificationType: NotificationType;
  title: string;
  message: string;
  link?: string;
  viewedAt?: string;
  createdAt: string;
  metadata: Record<string, unknown>;
};

export type NotificationRule = {
  id: number;
  userId: number;
  repositoryId: number;
  notificationType: NotificationType;
  enabled: boolean;
};

export type PrNotificationException = {
  id: number;
  userId: number;
  repositoryId: number;
  pullRequestId: number;
  notificationType: NotificationType;
  enabled: boolean;
};
