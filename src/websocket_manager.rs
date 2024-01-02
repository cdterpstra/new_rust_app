use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio::sync::{mpsc, broadcast};
use log::{info, error, debug};
use futures_util::{StreamExt, SinkExt};
use tokio::{spawn, time};
use rand::{Rng, rngs::StdRng, SeedableRng};
use crate::common::{StartPingMessage, BroadcastMessage};


// Define a structure for your start ping message


// Implement sending of the start ping message using an mpsc channel
async fn send_startping_message(
    sender: mpsc::Sender<StartPingMessage>,
    endpoint_name: String,
    timestamp: u128,
    ws_sender: mpsc::Sender<Message>, // Added parameter for WebSocket sender
) {
    let message = StartPingMessage {
        timestamp,
        endpoint_name,
        ws_sender // Pass this along
    };

    debug!("Sending start ping message: {:?}", message); // Debug before sending

    if let Err(e) = sender.send(message).await {
        error!("Failed to send start ping message: {:?}", e);
    } else {
        info!("Start ping message sent");
    }
}

async fn manage_connection(
    uri: String,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_sender: mpsc::Sender<StartPingMessage>,
) {
    let mut retry_delay = 1; // start with 1 second
    let mut rng = StdRng::from_entropy(); // using StdRng which is Send

    debug!("Starting manage_connection for {}", uri); // Debug at the start of connection management

    loop {
        match connect_async(&uri).await {
            Ok((ws_stream, _)) => {
                debug!("Connection established to {}", uri); // Debug on successful connection
                retry_delay = 1; // Reset retry delay upon successful connection

                let timestamp = chrono::Utc::now().timestamp_millis() as u128;

                let (mut write, mut read) = ws_stream.split();
                let (tx, mut rx) = mpsc::channel(32);

                send_startping_message(ping_sender.clone(), uri.clone(), timestamp, tx.clone()).await;

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
                            global_broadcaster.send(broadcast_msg).expect("Failed to broadcast message");
                            debug!("Broadcasted message from {}", uri); // Debug after broadcasting a message
                        },
                        Err(e) => {
                            error!("Error receiving ws message: {:?}", e);
                            break;
                        }
                    }
                }

                debug!("Connection lost to {}. Attempting to reconnect...", uri); // Debug on connection loss
            },
            Err(e) => {
                error!("Failed to connect to {}: {:?}", uri, e);
                debug!("Retrying connection to {} after {} seconds", uri, retry_delay); // Debug retry logic
            }
        }

        let jitter = rng.gen_range(0..6); // jitter of up to 5 seconds
        let sleep_time = std::cmp::min(retry_delay, 1024) + jitter; // capping the retry_delay at 1024 seconds
        time::sleep(time::Duration::from_secs(sleep_time)).await;
        retry_delay *= 2; // Double the retry delay for the next round
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_sender: mpsc::Sender<StartPingMessage>, // Adjusted to accept ping_sender from main.rs
) {
    debug!("Initializing WebSocket manager"); // Debug when starting manager

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        debug!("Spawning manage_connection for {}", uri); // Debug before spawning
        spawn(manage_connection(uri, global_broadcaster.clone(), ping_sender.clone()));
    }
}
