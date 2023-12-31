use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc::{Receiver, Sender}};
use log::{info, error, debug};
use crate::websocket_manager::Connection;

pub struct PingManager {
    connection_map: Arc<RwLock<HashMap<u128, Connection>>>,
    write_sender: Sender<String>,  // To send messages to write_task
}

impl PingManager {
    pub fn new(connection_map: Arc<RwLock<HashMap<u128, Connection>>>, write_sender: Sender<String>) -> Self {
        debug!("Initializing new PingManager with connection map.");
        // Log the pointer address and strong count of the connection_map upon PingManager creation
        debug!("PingManager connection_map Arc pointer address: {:?}", Arc::as_ptr(&connection_map));
        debug!("PingManager connection_map Arc strong count: {}", Arc::strong_count(&connection_map));
        Self { connection_map, write_sender }
    }

    pub async fn start(&self, mut signal_receiver: Receiver<String>) {
        info!("Ping manager started...");
        let connection_map = self.connection_map.clone();
        let write_sender = self.write_sender.clone(); // Clone the sender for sending pings

        tokio::spawn(async move {
            while let Some(message) = signal_receiver.recv().await {
                debug!("PingManager received message: {}", message);
                if let Some(connection_timestamp_str) = message.strip_prefix("Connection established for:") {
                    debug!("Detected connection establishment message, attempting to parse timestamp.");
                    match connection_timestamp_str.trim().parse::<u128>() {
                        Ok(connection_timestamp) => {
                            debug!("Parsed timestamp: {}, attempting to send ping.", connection_timestamp);
                            Self::send_ping(&connection_map, connection_timestamp, &write_sender).await; // Pass the write_sender to send_ping
                        },
                        Err(e) => {
                            error!("Failed to parse connection timestamp from message: {}. Error: {}", message, e);
                        },
                    }
                }
            }
        });
    }

    async fn send_ping(connection_map: &Arc<RwLock<HashMap<u128, Connection>>>, connection_timestamp: u128, write_sender: &Sender<String>) {
        debug!("Attempting to acquire read lock on connection_map to send ping to timestamp: {}", connection_timestamp);
        let read_map = connection_map.read().await;

        // Log the state of the connection_map right after acquiring the read lock
        debug!("Read lock acquired for timestamp: {}. Current state of connection_map: {:?}", connection_timestamp, read_map);
        debug!("Connection_map Arc pointer address at read lock: {:?}", Arc::as_ptr(connection_map));
        debug!("Connection_map Arc strong count at read lock: {}", Arc::strong_count(connection_map));

        if let Some(_connection) = read_map.get(&connection_timestamp) {
            debug!("Connection found for timestamp {}. Preparing to send ping.", connection_timestamp);
            let ping_message = "ping".to_string();
            if let Err(e) = write_sender.send(ping_message.clone()).await {
                error!("Failed to send ping to write task for connection {}: {}. Error: {}", connection_timestamp, ping_message, e);
            } else {
                debug!("Ping message sent to write task successfully for connection timestamp: {}", connection_timestamp);
            }
        } else {
            error!("No connection found for timestamp: {}", connection_timestamp);
            // Log the state of the connection_map at the time of this error
            debug!("Connection for timestamp not found. Detailed state of connection_map: {:?}", read_map);
        }
    }
}
