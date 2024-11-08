import { api } from "../api/api";

export function subscribeUser() {
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.ready.then((registration) => {
      const publicVapidKey =
        "BBxWgyzTUn1tmVdmImHR67ZWFUuvqY_l8UErBpkHRcaakYqV4TjUM1_el0P0M3rQYN-gzO2tsykLuOLMFXG8y50";

      registration.pushManager
        .subscribe({
          userVisibleOnly: true,
          applicationServerKey: urlBase64ToPaddedUint8Array(publicVapidKey),
        })
        .then((subscription) => {
          console.log("User is subscribed:", subscription);
          // Send subscription object to the server
          sendSubscriptionToServer(subscription);
        })
        .catch((err) => {
          console.log("Failed to subscribe the user: ", err);
        });
    });
  }
}

export function hasPushPermission() {
  return (
    "serviceWorker" in navigator &&
    "PushManager" in window &&
    Notification.permission === "granted"
  );
}

export function requestNotificationPermission(options?: {
  onGranted?: () => void;
  onDenied?: () => void;
  onNotSupported?: () => void;
}) {
  if ("Notification" in window) {
    Notification.requestPermission().then((permission) => {
      if (permission === "granted") {
        console.log("Notification permission granted.");
        options?.onGranted?.();
      } else {
        console.warn("Unable to get permission to notify.");
        options?.onDenied?.();
      }
    });
  } else {
    console.warn("This browser does not support notifications.");
    options?.onNotSupported?.();
  }
}

function sendSubscriptionToServer(subscription: PushSubscription) {
  api
    .post("notifications/subscribe", {
      json: subscription,
    })
    .then((response) => {
      if (!response.ok) {
        throw new Error("Bad status code from server.");
      }
    })
    .then((responseData) => {
      console.log("Subscription sent to server:", responseData);
    })
    .catch((err) => {
      console.log("Error sending subscription to server:", err);
    });
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
