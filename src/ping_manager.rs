use tokio::time::{self, Duration, Instant, sleep};
use tokio_tungstenite::tungstenite::protocol::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use log::{info, error, debug};
use uuid::Uuid;
use serde_json::{json, Value};
use colored;
use colored::Colorize;

use crate::BroadcastMessage;
use crate::common::WebSocketConnection;

pub async fn ping_manager(
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
    mut receiver: broadcast::Receiver<BroadcastMessage>,
) {
    // Map to track sent pings
    let sent_pings: Arc<RwLock<HashMap<String, Instant>>> = Arc::new(RwLock::new(HashMap::new()));

    send_ping(&connection_map, &sent_pings).await; // Send the first ping immediately

    let mut interval = time::interval(Duration::from_secs(15)); // Ping every 15 seconds

    // Introduce a small delay before entering the loop to send the first ping immediately
    sleep(Duration::from_secs(1)).await;

    loop {
        interval.tick().await; // Wait for next interval tick

        // Send a ping at each interval
        send_ping(&connection_map, &sent_pings).await;

        // Listen for the pong message in a non-blocking fashion
        while let Ok(msg) = receiver.try_recv() {
            // Deserialize the pong message from the WebSocket message
            if let Message::Text(text) = &msg.message {
                if let Ok(pong) = serde_json::from_str::<Value>(&text) {
                    // Check if pong message contains "ret_msg" with "success" and valid "req_id"
                    if pong["ret_msg"] == "pong" && pong["success"] == true{
                        if let Some(req_id) = pong["req_id"].as_str() {
                            // Remove the req_id from sent_pings if it matches
                            if sent_pings.write().await.remove(req_id).is_some() {
                                info!("Received valid pong for UUID: {}", req_id.bright_red());
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn send_ping(
    connection_map: &Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
    sent_pings: &Arc<RwLock<HashMap<String, Instant>>>,
) {
    let connections = connection_map.read().await; // Acquire read lock for the connection_map

    for (_, connection) in connections.iter() {
        let req_id = Uuid::new_v4().to_string(); // Create a unique UUID for each ping message

        // Store the sent ping's UUID
        sent_pings.write().await.insert(req_id.clone(), Instant::now());

        // Construct the ping message
        let ping_message_json_string = json!({
            "op": "ping",
            "args": null,
            "req_id": req_id,
        })
            .to_string();

        let ping_message = Message::Text(ping_message_json_string);

        match connection.sender.send(ping_message).await {
            Ok(_) => info!("Ping sent to endpoint {}", &connection.endpoint_name),
            Err(e) => error!("Failed to send ping to {}: {:?}", &connection.endpoint_name, e),
        }
    }
}
