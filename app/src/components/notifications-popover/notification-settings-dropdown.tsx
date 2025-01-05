import { notificationsMutations } from "@/lib/api/mutations/notifications";
import { notificationsQueries } from "@/lib/api/queries/notifications";
import {
  hasPushPermission,
  requestNotificationPermission,
} from "@/lib/notifications/web_push";
import { useQuery } from "@tanstack/react-query";
import dayjs from "dayjs";
import {
  Settings2,
  BellRing,
  BellOff,
  CheckCircle2,
  Bell,
  Trash2,
} from "lucide-react";
import { useState, useEffect } from "react";
import { toast } from "sonner";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../ui/dropdown-menu";
import { atomWithStorage } from "jotai/utils";
import { useAtom } from "jotai/react";

const deviceNamePersistedAtom = atomWithStorage<string | undefined>(
  "deviceName",
  undefined,
);

export function NotificationSettingsDropdown() {
  const [deviceName, setDeviceName] = useAtom(deviceNamePersistedAtom);
  const [browserPermission, setBrowserPermission] =
    useState<NotificationPermission>(hasPushPermission());

  const { data: isSubscribed } = useQuery(
    notificationsQueries.isSubscribed(deviceName),
  );
  const { mutate: subscribeToPush } = notificationsMutations.useSubscribeToPush(
    {
      onSuccess: () => {
        toast.success("Toki push notifications enabled for this device.");
      },
      onError: () => {
        toast.error(
          "Failed to enable Toki push notifications for this device.",
        );
      },
    },
  );

  const { data: pushSubscriptions = [] } = useQuery(
    notificationsQueries.pushSubscriptions(),
  );

  const { mutate: deletePushSubscription } =
    notificationsMutations.useDeletePushSubscription({
      onSuccess: () => {
        toast.success("Push subscription deleted.");
      },
      onError: () => {
        toast.error("Failed to delete push subscription.");
      },
    });

  const handleRequestPermission = () => {
    requestNotificationPermission({
      onGranted: () => {
        setBrowserPermission("granted");
      },
      onDenied: () => {
        setBrowserPermission("denied");
      },
      onNotSupported: () => {
        setBrowserPermission("denied");
      },
    });
  };

  const handleSubscribe = () => {
    if (browserPermission === "granted") {
      const name = prompt(
        "Enter a name for this device (this will help you identify it in the future)",
      );
      if (name) {
        setDeviceName(name);
        subscribeToPush({ deviceName: name });
      }
    }
  };

  useEffect(() => {
    setBrowserPermission(hasPushPermission());

    const permissionChangeHandler = () => {
      setBrowserPermission(hasPushPermission());
    };

    if ("permissions" in navigator) {
      navigator.permissions
        .query({ name: "notifications" })
        .then((permissionStatus) => {
          permissionStatus.addEventListener("change", permissionChangeHandler);
        });
    }

    return () => {
      if ("permissions" in navigator) {
        navigator.permissions
          .query({ name: "notifications" })
          .then((permissionStatus) => {
            permissionStatus.removeEventListener(
              "change",
              permissionChangeHandler,
            );
          });
      }
    };
  }, []);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          className="rounded-md p-1 opacity-50 transition-opacity hover:bg-muted hover:opacity-100"
          title="Notification settings"
        >
          <Settings2 className="size-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-[220px]">
        <div className="px-2 py-1.5 text-sm font-semibold">
          Notification Settings
        </div>
        <DropdownMenuSeparator />

        {/* Browser Permission Status */}
        <DropdownMenuItem
          onClick={handleRequestPermission}
          className="gap-2"
          disabled={browserPermission === "granted"}
        >
          {browserPermission === "granted" ? (
            <BellRing className="size-4" />
          ) : (
            <BellOff className="size-4" />
          )}
          <span className="text-xs">
            {browserPermission === "granted"
              ? "Browser notifications allowed"
              : "Allow browser notifications"}
          </span>
        </DropdownMenuItem>

        {/* Push Subscription Status */}
        <DropdownMenuItem
          onClick={handleSubscribe}
          className="gap-2"
          disabled={browserPermission !== "granted" || isSubscribed}
        >
          {isSubscribed ? (
            <CheckCircle2 className="size-4" />
          ) : (
            <Bell className="size-4" />
          )}
          <span className="text-xs">
            {isSubscribed
              ? "Toki notifications enabled"
              : "Enable Toki notifications"}
          </span>
        </DropdownMenuItem>
        {!!pushSubscriptions.length && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Subscribed devices</DropdownMenuLabel>
            {pushSubscriptions.map((subscription) => (
              <DropdownMenuItem
                key={subscription.id}
                className="group/item relative truncate text-xs"
                onClick={(e) => {
                  e.stopPropagation();
                  deletePushSubscription(subscription.id);
                }}
              >
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className="flex w-full items-center transition-all group-focus/item:pl-6">
                      <div className="absolute left-2 opacity-0 transition-all group-focus/item:opacity-100">
                        <Trash2 className="size-4 text-destructive" />
                      </div>
                      <span className="truncate">{subscription.device}</span>
                    </div>
                  </TooltipTrigger>
                  <TooltipContent side="right">
                    <div className="flex flex-col gap-1">
                      <p className="font-mono text-xs">
                        {subscription.device}
                        {deviceName === subscription.device && (
                          <span className="text-muted-foreground">
                            {" "}
                            (current device)
                          </span>
                        )}
                      </p>
                      <p>
                        Subscribed at{" "}
                        <span className="font-semibold">
                          {dayjs(subscription.createdAt).format(
                            "YYYY-MM-DD HH:mm",
                          )}
                        </span>
                      </p>
                    </div>
                  </TooltipContent>
                </Tooltip>
              </DropdownMenuItem>
            ))}
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
