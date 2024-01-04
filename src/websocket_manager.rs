// Import necessary crates and modules
use crate::common::{BroadcastMessage, StartPingMessage, Status, SubscriptionMessage};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info};
use rand::{rngs::StdRng, Rng, SeedableRng};
use tokio::sync::{broadcast, mpsc};
use tokio::{spawn, time};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, WebSocketStream};

// ===================================
// == Utility Functions Section ==
// ===================================

/// Send a generic message through an mpsc channel with logging.
async fn send_message<T: std::fmt::Debug>(
    sender: mpsc::Sender<T>,
    message: T,
    message_type: &str,
) {
    debug!("Sending {} message: {:?}", message_type, message);
    if let Err(e) = sender.send(message).await {
        error!("Failed to send {} message: {:?}", message_type, e);
    } else {
        info!("{} message sent", message_type);
    }
}

/// Create a StartPingMessage with given parameters
fn create_start_ping_message(
    endpoint_name: &str,
    timestamp: u128,
    ws_sender: mpsc::Sender<Message>,
) -> StartPingMessage {
    StartPingMessage {
        timestamp,
        endpoint_name: endpoint_name.to_owned(),
        ws_sender,
    }
}

/// Create a SubscriptionMessage with given parameters
fn create_subscription_message(
    endpoint_name: &str,
    timestamp: u128,
    ws_sender: mpsc::Sender<Message>,
) -> SubscriptionMessage {
    SubscriptionMessage {
        timestamp,
        endpoint_name: endpoint_name.to_owned(),
        ws_sender,
    }
}

// ===================================
// == WebSocket Management Section ==
// ===================================

/// Manages individual WebSocket connections.
/// It handles connection establishment, message sending, and reconnecting.
async fn manage_connection(
    uri: String,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_sender: mpsc::Sender<StartPingMessage>,
    status_receiver: broadcast::Receiver<Status>,
    subscription_sender: mpsc::Sender<SubscriptionMessage>,
) {
    let mut retry_delay = 1;
    let mut rng = StdRng::from_entropy();
    debug!("Starting connection management for {}", uri);

    // Listen for status messages in a separate task
    spawn(listen_for_status(status_receiver, uri.clone()));


    loop {
        match connect_async(&uri).await {
            Ok((ws_stream, _)) => {
                debug!("Connected to {}", uri);
                retry_delay = 1;
                handle_websocket_stream(
                    ws_stream,
                    &uri,
                    &global_broadcaster,
                    &ping_sender,
                    &subscription_sender,
                )
                    .await;
                debug!("Connection lost to {}. Attempting to reconnect...", uri);
            }
            Err(e) => {
                error!("Failed to connect to {}: {:?}", uri, e);
                debug!("Retrying connection to {} after {} seconds", uri, retry_delay);
            }
        }

        // Exponential backoff with jitter for reconnection attempts
        let jitter = rng.gen_range(0..6);
        let sleep_time = std::cmp::min(retry_delay, 1024) + jitter;
        time::sleep(time::Duration::from_secs(sleep_time)).await;
        retry_delay *= 2;
    }
}

/// Handles the WebSocket stream by sending ping and subscription messages
/// and listening for incoming messages to broadcast.
async fn handle_websocket_stream<S>(
    ws_stream: WebSocketStream<S>,
    uri: &String,
    global_broadcaster: &broadcast::Sender<BroadcastMessage>,
    ping_sender: &mpsc::Sender<StartPingMessage>,
    subscription_sender: &mpsc::Sender<SubscriptionMessage>,
)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let timestamp = chrono::Utc::now().timestamp_millis() as u128;
    let (mut write, mut read) = ws_stream.split();
    let (tx, mut rx) = mpsc::channel(32);

    // Send ping and subscription messages for this WebSocket connection
    send_message(
        ping_sender.clone(),
        create_start_ping_message(uri, timestamp, tx.clone()),
        "start ping",
    )
        .await;
    send_message(
        subscription_sender.clone(),
        create_subscription_message(uri, timestamp, tx.clone()),
        "subscription",
    )
        .await;

    // Handle sending messages from the channel to the WebSocket server
    spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = write.send(message).await {
                error!("Error sending ws message: {:?}", e);
                break;
            }
        }
    });

    // Handle receiving messages from the WebSocket server and broadcasting them
    while let Some(message) = read.next().await {
        match message {
            Ok(msg) => {
                let broadcast_msg = BroadcastMessage {
                    timestamp,
                    endpoint_name: uri.clone(),
                    message: msg,
                };
                if let Err(e) = global_broadcaster.send(broadcast_msg) {
                    error!("Failed to broadcast message: {:?}", e);
                }
                debug!("Broadcasted message from {}", uri);
            }
            Err(e) => {
                error!("Error receiving ws message: {:?}", e);
                break;
            }
        }
    }
}

/// Listens for Status messages specific to the current endpoint.
async fn listen_for_status(mut status_receiver: broadcast::Receiver<Status>, uri: String) {
    while let Ok(status) = status_receiver.recv().await {
        // Check if the status is intended for this endpoint
        if status.endpoint_name == uri {
            // Now filter based on the sending party
            match status.sending_party.as_ref() {
                "pingmanager" => {
                    debug!("Received pong status for: {} from {}", uri, status.endpoint_name);
                }
                "subscription_manager" => {
                    debug!("Received subscription status for: {} from {}", uri, status.endpoint_name);
                }
                _ => {
                    debug!("Received unknown status type for: {}", uri);
                }
            }
        } else {
            // Log that the message was received but not for this endpoint
            debug!(
                "Received status for {}, but listening for {}",
                status.endpoint_name, uri
            );
        }
    }
}


// ===================================
// == Application Entry Section ==
// ===================================

/// Main WebSocket manager, initializing connections for each endpoint.
pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_sender: mpsc::Sender<StartPingMessage>,
    status_receiver: broadcast::Receiver<Status>,
    subscription_sender: mpsc::Sender<SubscriptionMessage>,
) {
    debug!("Initializing WebSocket manager");
    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        spawn(manage_connection(
            uri,
            global_broadcaster.clone(),
            ping_sender.clone(),
            status_receiver.resubscribe(),
            subscription_sender.clone(),
        ));
    }
}
