use crate::common::{BroadcastMessage, StartPingMessage, Status, SubscriptionMessage};
use colored::Colorize;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info};
use rand::{rngs::StdRng, Rng, SeedableRng};
use tokio::sync::{broadcast, mpsc};
use tokio::{spawn, time};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

async fn send_startping_message(
    sender: mpsc::Sender<StartPingMessage>,
    endpoint_name: String,
    timestamp: u128,
    ws_sender: mpsc::Sender<Message>,
) {
    let message = StartPingMessage {
        timestamp,
        endpoint_name,
        ws_sender,
    };

    debug!("Sending start ping message: {:?}", message);

    if let Err(e) = sender.send(message).await {
        error!("Failed to send start ping message: {:?}", e);
    } else {
        info!("Start ping message sent");
    }
}

async fn send_subscription_message(
    sender: mpsc::Sender<SubscriptionMessage>,
    endpoint_name: String,
    timestamp: u128,
    ws_sender: mpsc::Sender<Message>,
) {
    let message = SubscriptionMessage {
        timestamp,
        endpoint_name,
        ws_sender,
    };

    debug!("Sending start subscription message: {:?}", message);

    if let Err(e) = sender.send(message).await {
        error!("Failed to send start subscription message: {:?}", e);
    } else {
        info!("Start subscription message sent");
    }
}

async fn manage_connection(
    uri: String,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_sender: mpsc::Sender<StartPingMessage>,
    mut status_receiver: broadcast::Receiver<Status>,
    subscription_sender: mpsc::Sender<SubscriptionMessage>,
) {
    let mut retry_delay = 1;
    let mut rng = StdRng::from_entropy();

    debug!("Starting manage_connection for {}", uri);

    // Spawn a new task to listen for PongStatus messages
    let uri_clone = uri.clone(); // Clone URI to use in the spawned task
    spawn(async move {
        while let Ok(pong_status) = status_receiver.recv().await {
            // Check if the PongStatus message is for the current endpoint
            if pong_status.endpoint_name == uri_clone && pong_status.sending_party == "pingmanager" {
                debug!(
                    "{} {} {:?}",
                    "Received pong status for:".green(),
                    uri_clone.green(),
                    pong_status
                );
                // Process the PongStatus message as needed
            } else {
                debug!(
                    "{} {} {} {}",
                    "This is endpoint:".red(),
                    uri_clone.red(),
                    "received PongStatus for endpoint:".red(),
                    pong_status.endpoint_name.red()
                );
            }
        }
    });

    loop {
        match connect_async(&uri).await {
            Ok((ws_stream, _)) => {
                debug!("Connection established to {}", uri);
                retry_delay = 1;

                let timestamp = chrono::Utc::now().timestamp_millis() as u128;

                let (mut write, mut read) = ws_stream.split();
                let (tx, mut rx) = mpsc::channel(32);

                send_startping_message(ping_sender.clone(), uri.clone(), timestamp, tx.clone())
                    .await;

                send_subscription_message(subscription_sender.clone(), uri.clone(), timestamp, tx.clone())
                    .await;

                spawn(async move {
                    while let Some(message) = rx.recv().await {
                        if let Err(e) = write.send(message).await {
                            error!("Error sending ws message: {:?}", e);
                            break;
                        }
                    }
                });



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

                debug!("Connection lost to {}. Attempting to reconnect...", uri);
            }
            Err(e) => {
                error!("Failed to connect to {}: {:?}", uri, e);
                debug!(
                    "Retrying connection to {} after {} seconds",
                    uri, retry_delay
                );
            }
        }

        let jitter = rng.gen_range(0..6);
        let sleep_time = std::cmp::min(retry_delay, 1024) + jitter;
        time::sleep(time::Duration::from_secs(sleep_time)).await;
        retry_delay *= 2;
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_sender: mpsc::Sender<StartPingMessage>,
    status_receiver: broadcast::Receiver<Status>,
    subscription_sender: mpsc::Sender<SubscriptionMessage>
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
