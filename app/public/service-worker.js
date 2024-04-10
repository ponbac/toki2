self.addEventListener("push", (event) => {
  console.log("recieved push data", event.data.text());
  const data = event.data.json();
  const { title, body, icon } = data;

  event.waitUntil(
    self.registration.showNotification(title, {
      body,
      icon,
      data: data.url,
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  // eslint-disable-next-line no-undef
  event.waitUntil(clients.openWindow(event.notification.data || "/"));
});
