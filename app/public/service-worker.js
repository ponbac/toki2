self.addEventListener("push", (event) => {
  const data = event.data.json();
  const { title, body, icon } = data;

  event.waitUntil(
    self.registration.showNotification(title, {
      body,
      icon,
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  // eslint-disable-next-line no-undef
  event.waitUntil(clients.openWindow("/"));
});
