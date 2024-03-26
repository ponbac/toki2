import { CmdK } from "@/components/cmd-k";
import { LoadingSpinner } from "@/components/loading-spinner";
import { SideNavWrapper } from "@/components/side-nav";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api } from "@/lib/api/api";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import React from "react";
import { Suspense } from "react";

export const Route = createFileRoute("/_layout")({
  component: LayoutComponent,
});

function LayoutComponent() {
  React.useEffect(() => {
    if ("Notification" in window) {
      Notification.requestPermission().then((permission) => {
        if (permission === "granted") {
          console.log("Notification permission granted.");
          // Proceed with subscribing the user
          // subscribeUser();
        } else {
          console.log("Unable to get permission to notify.");
        }
      });
    }
  }, []);

  return (
    <TooltipProvider delayDuration={0}>
      <SideNavWrapper
        accounts={[
          {
            email: "root@ponbac.xyz",
            label: "Root",
            icon: "👑",
          },
        ]}
        navCollapsedSize={2}
        defaultCollapsed={true}
        className="flex h-full min-h-screen w-full flex-col"
      >
        <button
          onClick={subscribeUser}
          className="rounded-md bg-orange-500 p-2"
        >
          SUBSCRIBE!
        </button>
        <Suspense fallback={<FullscreenLoading />}>
          <Outlet />
        </Suspense>
      </SideNavWrapper>
      <Toaster />
      <CmdK />
    </TooltipProvider>
  );
}

function FullscreenLoading() {
  return (
    <div className="flex min-h-screen w-full items-center justify-center">
      <LoadingSpinner className="size-8" />
    </div>
  );
}

function subscribeUser() {
  if ("serviceWorker" in navigator) {
    navigator.serviceWorker.ready.then((registration) => {
      const publicVapidKey =
        "BBxWgyzTUn1tmVdmImHR67ZWFUuvqY_l8UErBpkHRcaakYqV4TjUM1_el0P0M3rQYN-gzO2tsykLuOLMFXG8y50";

      registration.pushManager
        .subscribe({
          userVisibleOnly: true,
          applicationServerKey: urlBase64ToUint8Array(publicVapidKey),
        })
        .then((subscription) => {
          console.log("User is subscribed:", subscription);
          // Send the subscription object to your server
          sendSubscriptionToServer(subscription);
        })
        .catch((err) => {
          console.log("Failed to subscribe the user: ", err);
        });
    });
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
      return response.json();
    })
    .then((responseData) => {
      console.log("Subscription sent to server:", responseData);
    })
    .catch((err) => {
      console.log("Error sending subscription to server:", err);
    });
}

function urlBase64ToUint8Array(base64String: string) {
  const padding = "=".repeat((4 - (base64String.length % 4)) % 4);
  const base64 = (base64String + padding).replace(/-/g, "+").replace(/_/g, "/");
  const rawData = atob(base64);
  const outputArray = new Uint8Array(rawData.length);
  for (let i = 0; i < rawData.length; ++i) {
    outputArray[i] = rawData.charCodeAt(i);
  }
  return outputArray;
}
