use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, broadcast};
use log::{info, error};
use futures_util::{StreamExt, SinkExt};
use std::collections::HashMap;
use tokio::{spawn, time};
use rand::{Rng, rngs::StdRng, SeedableRng}; // adjusted import
use crate::common::WebSocketConnection;

#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub message: Message,
}

async fn manage_connection(
    uri: String,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>
) {
    let mut retry_delay = 1; // start with 1 second
    let mut rng = StdRng::from_entropy(); // using StdRng which is Send

    loop { // Reconnection loop
        match connect_async(&uri).await {
            Ok((ws_stream, _)) => {
                info!("Connected to {}", uri);
                retry_delay = 1; // Reset retry delay upon successful connection
                let (tx, mut rx) = mpsc::channel(32);

                let timestamp = chrono::Utc::now().timestamp_millis() as u128;
                connection_map.write().await.insert(timestamp, WebSocketConnection {
                    sender: tx.clone(),
                    endpoint_name: uri.clone(),
                });

                let (mut write, mut read) = ws_stream.split();

                let write_handle = spawn(async move {
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
                        },
                        Err(e) => {
                            error!("Error receiving ws message: {:?}", e);
                            break;
                        }
                    }
                }

                // If you reach here, the connection is closed or errored out
                connection_map.write().await.remove(&timestamp); // Remove the connection from the map
                write_handle.abort(); // Stop sending messages
                info!("Connection lost to {}. Attempting to reconnect...", uri);
            },
            Err(e) => {
                error!("Failed to connect to {}: {:?}", uri, e);
            }
        }

        // Exponential backoff calculation
        let jitter = rng.gen_range(0..6); // jitter of up to 5 seconds
        let sleep_time = std::cmp::min(retry_delay, 1024) + jitter; // capping the retry_delay at 1024 seconds
        time::sleep(time::Duration::from_secs(sleep_time)).await;
        retry_delay *= 2; // Double the retry delay for the next round
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
    global_broadcaster: broadcast::Sender<BroadcastMessage>
) {
    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        spawn(manage_connection(uri, global_broadcaster.clone(), connection_map.clone()));
    }
}
