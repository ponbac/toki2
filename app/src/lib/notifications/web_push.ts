import { api } from "../api/api";

export async function subscribeUser() {
  if ("serviceWorker" in navigator) {
    try {
      const registration = await navigator.serviceWorker.ready;
      const publicVapidKey =
        "BBxWgyzTUn1tmVdmImHR67ZWFUuvqY_l8UErBpkHRcaakYqV4TjUM1_el0P0M3rQYN-gzO2tsykLuOLMFXG8y50";

      const subscription = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: urlBase64ToPaddedUint8Array(publicVapidKey),
      });

      console.log("User is subscribed:", subscription);
      await sendSubscriptionToServer(subscription);
    } catch (err) {
      console.log("Failed to subscribe the user: ", err);
      throw err;
    }
  }

  return "OK" as const;
}
export function hasPushPermission(): NotificationPermission {
  if (
    !("serviceWorker" in navigator) ||
    !("PushManager" in window) ||
    !("Notification" in window)
  ) {
    return "denied";
  }

  return Notification.permission;
}

export async function requestNotificationPermission(options?: {
  onGranted?: () => void;
  onDenied?: () => void;
  onNotSupported?: () => void;
}) {
  if ("Notification" in window) {
    try {
      const permission = await Notification.requestPermission();
      if (permission === "granted") {
        console.log("Notification permission granted.");
        options?.onGranted?.();
      } else {
        console.warn("Unable to get permission to notify.");
        options?.onDenied?.();
      }
    } catch (err) {
      console.error("Error requesting notification permission:", err);
      options?.onDenied?.();
      throw err;
    }
  } else {
    console.warn("This browser does not support notifications.");
    options?.onNotSupported?.();
  }
}

async function sendSubscriptionToServer(subscription: PushSubscription) {
  try {
    const response = await api.post("notifications/subscribe", {
      json: subscription,
    });

    if (!response.ok) {
      throw new Error("Bad status code from server.");
    }

    console.log("Subscription sent to server successfully");
    return response;
  } catch (err) {
    console.error("Error sending subscription to server:", err);
    throw err;
  }
}

function urlBase64ToPaddedUint8Array(base64String: string) {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const rawData = atob(base64);
  const outputArray = new Uint8Array(rawData.length);
  for (let i = 0; i < rawData.length; ++i) {
    outputArray[i] = rawData.charCodeAt(i);
  }
  return outputArray;
}
