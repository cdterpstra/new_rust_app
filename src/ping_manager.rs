use tokio::time::{self, Duration};
use tokio_tungstenite::tungstenite::protocol::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, error};
use uuid::Uuid;  // For generating UUID
use serde_json::json;  // For creating JSON
use crate::common::WebSocketConnection;

pub async fn ping_manager(
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
) {
    let mut interval = time::interval(Duration::from_secs(15));  // Ping every 15 seconds
    loop {
        interval.tick().await;  // Wait for next interval tick

        // Acquire read lock for the connection_map
        let connections = connection_map.read().await;

        for (_, connection) in connections.iter() {
            // Create a unique UUID for each ping message
            let req_id = Uuid::new_v4();

            // Construct the ping message as a JSON string directly
            let ping_message_json_string = json!({
                "op": "ping",
                "args": null,
                "req_id": req_id.to_string(),
            }).to_string();

            // Convert the JSON string to a WebSocket text message
            let ping_message = Message::Text(ping_message_json_string);

            // Clone the sender for the spawned task
            let sender = connection.sender.clone();
            let endpoint_name = connection.endpoint_name.clone();

            // Send the ping message through each connection's sender channel
            match sender.send(ping_message).await {
                Ok(_) => info!("Ping sent to endpoint {}", &endpoint_name),
                Err(e) => error!("Failed to send ping to {}: {:?}", &endpoint_name, e),
            }
        }

        // No need to sleep here as interval.tick() already waits for 15 seconds.
    }
}
