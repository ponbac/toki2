import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions } from "./mutations";
import { notificationsQueries } from "../queries/notifications";
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

function useMarkNotificationViewed(options?: DefaultMutationOptions<number>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "view"],
    mutationFn: (notificationId: number) =>
      api.post(`notifications/${notificationId}/view`),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: ["notifications", "list"] });
      options?.onSuccess?.(data, vars, ctx);
    },
  });
}

function useMarkAllNotificationsViewed(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["notifications", "view-all"],
    mutationFn: () => api.post("notifications/view-all"),
    ...options,
    onSuccess: (data, vars, ctx) => {
      queryClient.invalidateQueries({ queryKey: ["notifications", "list"] });
      options?.onSuccess?.(data, vars, ctx);
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
      queryClient.invalidateQueries({ queryKey: ["notifications", "list"] });
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
