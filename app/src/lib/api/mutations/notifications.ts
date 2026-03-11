import {
  QueryClient,
  QueryKey,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { Notification, notificationsQueries } from "../queries/notifications";
import { subscribeUser } from "@/lib/notifications/web_push";

export enum NotificationType {
  PrClosed = "PrClosed",
  ThreadAdded = "ThreadAdded",
  ThreadUpdated = "ThreadUpdated",
  CommentMentioned = "CommentMentioned",
}

export const notificationsMutations = {
  useMarkNotificationViewed,
  useMarkAllNotificationsViewed,
  useDeleteNotification,
  useUpdatePreferences,
  useSetPrException,
  useRemovePrException,
  useSubscribeToPush,
  useDeletePushSubscription,
};

const notificationsListQueryKey = ["notifications", "list"] as const;

type NotificationsListOptions = {
  includeViewed: boolean;
  maxAgeDays: number;
};

type NotificationsListSnapshot = Array<
  [QueryKey, Array<Notification> | undefined]
>;

type NotificationsMutationContext = {
  optionsContext: unknown;
  previousLists: NotificationsListSnapshot;
};

function snapshotNotificationsLists(
  queryClient: QueryClient,
): NotificationsListSnapshot {
  return queryClient.getQueriesData<Array<Notification>>({
    queryKey: notificationsListQueryKey,
  });
}

function restoreNotificationsLists(
  queryClient: QueryClient,
  snapshots: NotificationsListSnapshot,
) {
  for (const [queryKey, data] of snapshots) {
    queryClient.setQueryData<Array<Notification> | undefined>(queryKey, data);
  }
}

function getNotificationsListOptions(
  queryKey: QueryKey,
): NotificationsListOptions | undefined {
  const options = queryKey[2];

  if (!options || typeof options !== "object") {
    return undefined;
  }

  const maybeOptions = options as Partial<NotificationsListOptions>;

  if (
    typeof maybeOptions.includeViewed !== "boolean" ||
    typeof maybeOptions.maxAgeDays !== "number"
  ) {
    return undefined;
  }

  return maybeOptions as NotificationsListOptions;
}

function updateNotificationsLists(
  queryClient: QueryClient,
  updater: (
    notifications: Array<Notification>,
    options: NotificationsListOptions | undefined,
  ) => Array<Notification>,
) {
  for (const [queryKey, notifications] of snapshotNotificationsLists(
    queryClient,
  )) {
    if (!notifications) continue;

    queryClient.setQueryData<Array<Notification>>(queryKey, (current) =>
      current
        ? updater(current, getNotificationsListOptions(queryKey))
        : current,
    );
  }
}

async function optimisticUpdateNotificationsLists<TVars>(
  queryClient: QueryClient,
  vars: TVars,
  onMutate: ((vars: TVars) => Promise<unknown> | unknown) | undefined,
  updater: (
    notifications: Array<Notification>,
    options: NotificationsListOptions | undefined,
  ) => Array<Notification>,
): Promise<NotificationsMutationContext> {
  await queryClient.cancelQueries({
    queryKey: notificationsListQueryKey,
  });

  const previousLists = snapshotNotificationsLists(queryClient);
  updateNotificationsLists(queryClient, updater);
  const optionsContext = await onMutate?.(vars);

  return {
    optionsContext,
    previousLists,
  };
}

function useMarkNotificationViewed(options?: DefaultMutationOptions<number>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "view"],
    mutationFn: (notificationId: number) =>
      api.post(`notifications/${notificationId}/view`),
    ...options,
    onMutate: async (notificationId) => {
      const viewedAt = new Date().toISOString();

      return optimisticUpdateNotificationsLists(
        queryClient,
        notificationId,
        options?.onMutate,
        (notifications, queryOptions) => {
          if (queryOptions?.includeViewed === false) {
            return notifications.filter(
              (notification) => notification.id !== notificationId,
            );
          }

          return notifications.map((notification) =>
            notification.id === notificationId
              ? {
                  ...notification,
                  viewedAt,
                }
              : notification,
          );
        },
      );
    },
    onSuccess: (data, vars, ctx) => {
      options?.onSuccess?.(data, vars, ctx?.optionsContext);
    },
    onError: (error, vars, ctx) => {
      restoreNotificationsLists(queryClient, ctx?.previousLists ?? []);
      options?.onError?.(error, vars, ctx?.optionsContext);
    },
    onSettled: (data, error, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: notificationsListQueryKey });
      options?.onSettled?.(data, error, vars, ctx?.optionsContext);
    },
  });
}

function useMarkAllNotificationsViewed(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "view-all"],
    mutationFn: () => api.post("notifications/view-all"),
    ...options,
    onMutate: async () => {
      const viewedAt = new Date().toISOString();

      return optimisticUpdateNotificationsLists(
        queryClient,
        undefined,
        options?.onMutate,
        (notifications, queryOptions) => {
          if (queryOptions?.includeViewed === false) {
            return [];
          }

          return notifications.map((notification) =>
            notification.viewedAt
              ? notification
              : {
                  ...notification,
                  viewedAt,
                },
          );
        },
      );
    },
    onSuccess: (data, vars, ctx) => {
      options?.onSuccess?.(data, vars, ctx?.optionsContext);
    },
    onError: (error, vars, ctx) => {
      restoreNotificationsLists(queryClient, ctx?.previousLists ?? []);
      options?.onError?.(error, vars, ctx?.optionsContext);
    },
    onSettled: (data, error, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: notificationsListQueryKey });
      options?.onSettled?.(data, error, vars, ctx?.optionsContext);
    },
  });
}

function useDeleteNotification(options?: DefaultMutationOptions<number>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "delete"],
    mutationFn: (notificationId: number) =>
      api.delete(`notifications/${notificationId}`),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: notificationsListQueryKey });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

type PreferencePayload = {
  repositoryId: number;
  rule: {
    id: number;
    userId: number;
    repositoryId: number;
    notificationType: NotificationType;
    enabled: boolean;
    pushEnabled: boolean;
  };
};

function useUpdatePreferences(
  options?: DefaultMutationOptions<PreferencePayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "preferences", "update"],
    mutationFn: ({ repositoryId, rule }: PreferencePayload) =>
      api.post(`notifications/preferences/${repositoryId}`, {
        json: rule,
      }),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({
        queryKey: notificationsQueries.preferences(vars.repositoryId).queryKey,
      });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

type PrExceptionPayload = {
  repositoryId: number;
  pullRequestId: number;
  exception: {
    repositoryId: number;
    pullRequestId: number;
    notificationType: NotificationType;
    enabled: boolean;
  };
};

function useSetPrException(
  options?: DefaultMutationOptions<PrExceptionPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "exceptions", "set"],
    mutationFn: ({
      repositoryId,
      pullRequestId,
      exception,
    }: PrExceptionPayload) =>
      api.post(
        `notifications/repositories/${repositoryId}/pull-requests/${pullRequestId}/exceptions`,
        {
          json: exception,
        },
      ),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({
        queryKey: notificationsQueries.prExceptions(
          vars.repositoryId,
          vars.pullRequestId,
        ).queryKey,
      });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

type RemoveExceptionPayload = {
  repositoryId: number;
  pullRequestId: number;
  notificationType: NotificationType;
};

function useRemovePrException(
  options?: DefaultMutationOptions<RemoveExceptionPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "exceptions", "remove"],
    mutationFn: ({
      repositoryId,
      pullRequestId,
      notificationType,
    }: RemoveExceptionPayload) =>
      api.delete(
        `notifications/repositories/${repositoryId}/pull-requests/${pullRequestId}/exceptions/${notificationType}`,
      ),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({
        queryKey: notificationsQueries.prExceptions(
          vars.repositoryId,
          vars.pullRequestId,
        ).queryKey,
      });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

type SubscribePayload = {
  deviceName: string | undefined;
};

function useSubscribeToPush(
  options?: DefaultMutationOptions<SubscribePayload, "OK">,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "subscribe"],
    mutationFn: ({ deviceName }: SubscribePayload) => subscribeUser(deviceName),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({
        queryKey: notificationsQueries.pushSubscriptions().queryKey,
      });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useDeletePushSubscription(options?: DefaultMutationOptions<number>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "push-subscriptions", "delete"],
    mutationFn: (id: number) =>
      api.delete(`notifications/push-subscriptions/${id}`),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({
        queryKey: notificationsQueries.pushSubscriptions().queryKey,
      });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}
