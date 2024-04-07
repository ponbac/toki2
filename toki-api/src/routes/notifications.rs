use std::{borrow::Cow, net::SocketAddr, ops::ControlFlow};

use crate::repositories::PushSubscriptionRepository;
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use axum_extra::{headers, TypedHeader};
use futures::{SinkExt, StreamExt};
use tracing::instrument;

use crate::{
    app_state::AppState, auth::AuthSession, domain::PushNotification,
    repositories::NewPushSubscription,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscribe", post(subscribe))
        .route("/test-push", post(test_push))
        .route("/ws", get(ws_handler))
}

#[instrument(name = "ws_handler", skip(auth_session, ws, user_agent, app_state))]
async fn ws_handler(
    auth_session: AuthSession,
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(app_state): State<AppState>,
) -> impl IntoResponse {
    let user_id = auth_session.user.expect("user not found").id;
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        "unknown user agent".to_string()
    };
    tracing::info!(
        "WebSocket connection from user_id={} user_agent={} addr={}",
        user_id,
        user_agent,
        addr
    );

    ws.on_upgrade(move |socket| handle_socket(socket, addr))
}

/// Actual websocket statemachine (one will be spawned per connection)
async fn handle_socket(mut socket: WebSocket, who: SocketAddr) {
    //send a ping (unsupported by some browsers) just to kick things off and get a response
    if socket.send(Message::Ping(vec![1, 2, 3])).await.is_ok() {
        println!("Pinged {who}...");
    } else {
        println!("Could not send ping {who}!");
        // no Error here since the only thing we can do is to close the connection.
        // If we can not send messages, there is no way to salvage the statemachine anyway.
        return;
    }

    // receive single message from a client (we can either receive or send with socket).
    // this will likely be the Pong for our Ping or a hello message from client.
    // waiting for message from a client will block this task, but will not block other client's
    // connections.
    if let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if process_message(msg, who).is_break() {
                return;
            }
        } else {
            println!("client {who} abruptly disconnected");
            return;
        }
    }

    // Since each client gets individual statemachine, we can pause handling
    // when necessary to wait for some external event (in this case illustrated by sleeping).
    // Waiting for this client to finish getting its greetings does not prevent other clients from
    // connecting to server and receiving their greetings.
    for i in 1..5 {
        if socket
            .send(Message::Text(format!("Hi {i} times!")))
            .await
            .is_err()
        {
            println!("client {who} abruptly disconnected");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // By splitting socket we can send and receive at the same time. In this example we will send
    // unsolicited messages to client based on some sort of server's internal event (i.e .timer).
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task that will push several messages to the client (does not matter what client does)
    let mut send_task = tokio::spawn(async move {
        let n_msg = 20;
        for i in 0..n_msg {
            // In case of any websocket error, we exit.
            if sender
                .send(Message::Text(format!("Server message {i} ...")))
                .await
                .is_err()
            {
                return i;
            }

            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        println!("Sending close to {who}...");
        if let Err(e) = sender
            .send(Message::Close(Some(CloseFrame {
                code: axum::extract::ws::close_code::NORMAL,
                reason: Cow::from("Goodbye"),
            })))
            .await
        {
            println!("Could not send Close due to {e}, probably it is ok?");
        }
        n_msg
    });

    // This second task will receive messages from client and print them on server console
    let mut recv_task = tokio::spawn(async move {
        let mut cnt = 0;
        while let Some(Ok(msg)) = receiver.next().await {
            cnt += 1;
            // print message and break if instructed to do so
            if process_message(msg, who).is_break() {
                break;
            }
        }
        cnt
    });

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        rv_a = (&mut send_task) => {
            match rv_a {
                Ok(a) => println!("{a} messages sent to {who}"),
                Err(a) => println!("Error sending messages {a:?}")
            }
            recv_task.abort();
        },
        rv_b = (&mut recv_task) => {
            match rv_b {
                Ok(b) => println!("Received {b} messages"),
                Err(b) => println!("Error receiving messages {b:?}")
            }
            send_task.abort();
        }
    }

    // returning from the handler closes the websocket connection
    println!("Websocket context {who} destroyed");
}

/// helper to print contents of messages to stdout. Has special treatment for Close.
fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
        }
        Message::Binary(d) => {
            println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow sent close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            println!(">>> {who} sent pong with {v:?}");
        }
        // You should never need to manually handle Message::Ping, as axum's websocket library
        // will do so for you automagically by replying with Pong and copying the v according to
        // spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
        }
    }
    ControlFlow::Continue(())
}

#[instrument(name = "subscribe", skip(auth_session, app_state))]
async fn subscribe(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<web_push::SubscriptionInfo>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();

    let new_push_subscription = NewPushSubscription {
        user_id,
        device: "NOT IMPLEMENTED".to_string(),
        endpoint: body.endpoint,
        auth: body.keys.auth,
        p256dh: body.keys.p256dh,
    };

    push_subscription_repo
        .upsert_push_subscription(new_push_subscription)
        .await
        .map_err(|e| {
            tracing::error!("Failed to upsert push subscription: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to upsert push subscription".to_string(),
            )
        })?;

    Ok(StatusCode::OK)
}

#[instrument(name = "test_push", skip(app_state))]
async fn test_push(State(app_state): State<AppState>) -> Result<StatusCode, (StatusCode, String)> {
    let push_subscription_repo = app_state.push_subscriptions_repo.clone();
    let subscribers = push_subscription_repo
        .get_push_subscriptions()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get push subscriptions: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get push subscriptions".to_string(),
            )
        })?;

    let content = PushNotification::new(
        "Hello, World!",
        "This is a test notification",
        Some("https://ponbac.xyz"),
        None,
    );
    for subscriber in subscribers {
        let push_message = content
            .to_web_push_message(&subscriber.as_subscription_info())
            .map_err(|e| {
                tracing::error!("Failed to create push message: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to create push message".to_string(),
                )
            })?;

        app_state
            .push_notification(push_message)
            .await
            .map_err(|e| {
                tracing::error!("Failed to send notification: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to send notification".to_string(),
                )
            })?;
    }

    Ok(StatusCode::OK)
}
