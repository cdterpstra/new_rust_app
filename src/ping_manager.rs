// src/ping_manager.rs

use uuid::Uuid;
use serde_json::json;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc::{Receiver, Sender}};
use tokio::time::{self, Duration};
use log::{info, error, debug};
use crate::websocket_manager::Connection;
use crate::common::ConnectionMessage;

pub struct PingManager {
    connection_map: Arc<RwLock<HashMap<u128, Connection>>>,
    global_sender: Sender<ConnectionMessage>,
}

impl PingManager {
    pub fn new(connection_map: Arc<RwLock<HashMap<u128, Connection>>>, global_sender: Sender<ConnectionMessage>) -> Self {
        debug!("Initializing new PingManager with connection map.");
        debug!("PingManager connection_map Arc pointer address: {:?}", Arc::as_ptr(&connection_map));
        debug!("PingManager connection_map Arc strong count: {}", Arc::strong_count(&connection_map));
        Self { connection_map, global_sender }
    }

    pub async fn start(&self, mut signal_receiver: Receiver<String>) {
        info!("Ping manager started...");
        let connection_map = self.connection_map.clone();
        let global_sender = self.global_sender.clone();  // Clone the global sender for sending pings

        tokio::spawn(async move {
            while let Some(message) = signal_receiver.recv().await {
                debug!("PingManager received message: {}", message);
                if let Some(connection_timestamp_str) = message.strip_prefix("Connection established for:") {
                    debug!("Detected connection establishment message, attempting to parse timestamp.");
                    if let Ok(connection_timestamp) = connection_timestamp_str.trim().parse::<u128>() {
                        debug!("Parsed timestamp: {}, starting ping loop.", connection_timestamp);
                        let connection_map_clone = connection_map.clone();
                        let global_sender_clone = global_sender.clone();

                        // Start a new asynchronous task for sending pings for each connection
                        tokio::spawn(async move {
                            Self::send_pings(connection_map_clone, connection_timestamp, &global_sender_clone).await;
                        });
                    } else {
                        error!("Failed to parse connection timestamp from message: {}", message);
                    }
                }
            }
        });
    }

    async fn send_pings(connection_map: Arc<RwLock<HashMap<u128, Connection>>>, connection_timestamp: u128, global_sender: &Sender<ConnectionMessage>) {
        let ping_interval = Duration::from_secs(15);
        loop {
            let connection_exists = {
                let read_map = connection_map.read().await;
                read_map.get(&connection_timestamp).is_some()
            };

            if connection_exists {
                let req_id = Uuid::new_v4().to_string();
                let ping_message = ConnectionMessage {
                    endpoint: None,
                    connection_timestamp,
                    message: json!({
                        "req_id": req_id,
                        "op": "ping"
                    }).to_string(),
                };

                let endpoint_name = {
                    let read_map = connection_map.read().await;
                    read_map.get(&connection_timestamp)
                        .map(|c| c.endpoint_name.clone())
                        .unwrap_or_else(|| "Unknown".to_string())
                };

                if let Err(_) = global_sender.send(ping_message).await {
                    error!("Failed to send ping for connection {}", connection_timestamp);
                } else {
                    debug!("Ping message sent successfully for connection timestamp {} to endpoint '{}'", connection_timestamp, endpoint_name);
                }

                // Waiting for the next ping interval before sending the next ping
                time::sleep(ping_interval).await;
            } else {
                debug!("Stopping ping loop: no connection found for timestamp {}", connection_timestamp);
                break;
            }
        }
    }
}

// Ensure all paths and module names used are correctly referenced and used.
