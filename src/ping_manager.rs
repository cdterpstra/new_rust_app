use tokio::time::{self, Duration, sleep};
use tokio_tungstenite::tungstenite::protocol::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, error};
use uuid::Uuid;
use serde_json::json;
use crate::common::WebSocketConnection;

pub async fn ping_manager(
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
) {
    // Send the first ping immediately
    send_ping(&connection_map).await;

    let mut interval = time::interval(Duration::from_secs(15)); // Ping every 15 seconds

    // Introduce a small delay before entering the loop to send the first ping immediately
    sleep(Duration::from_secs(1)).await;

    loop {
        interval.tick().await; // Wait for next interval tick

        // Send a ping at each interval
        send_ping(&connection_map).await;
    }
}

async fn send_ping(
    connection_map: &Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
) {
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
        })
            .to_string();

        // Convert the JSON string to a WebSocket text message
        let ping_message = Message::Text(ping_message_json_string.clone());

        // Clone the sender for the spawned task
        let sender = connection.sender.clone();
        let endpoint_name = connection.endpoint_name.clone();

        // Send the ping message through each connection's sender channel
        match sender.send(ping_message).await {
            Ok(_) => info!("Ping sent to endpoint {}", &endpoint_name),
            Err(e) => error!("Failed to send ping to {}: {:?}", &endpoint_name, e),
        }
    }
}
