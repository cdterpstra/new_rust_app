// ping_manager.rs
use serde_json::Value;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use log::{debug, error, trace};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;
use crate::websocket_manager::MyMessage;


pub async fn start_pinging(mut write: mpsc::Sender<MyMessage>, mut read: mpsc::Receiver<MyMessage>) {
    let mut interval = time::interval(Duration::from_secs(15));
    while let _ = interval.tick().await {
        let req_id = Uuid::new_v4().to_string();
        let ping_message = json!({
            "op": "ping",
            "args": null,
            "req_id": req_id,
        })
            .to_string();

        debug!("Sending ping message: {}", ping_message);

        // Constructing a text WebSocket message
        let ping_ws_message = Message::Text(ping_message); // This is the actual WebSocket message

        // Wrapping it into your MyMessage structure
        let my_ping_message = MyMessage {
            timestamp: chrono::Utc::now().timestamp_millis() as u128, // Assuming you keep track of timestamp
            endpoint_name: "YourEndpointName".to_string(), // Adapt as necessary
            message: ping_ws_message, // The actual WebSocket message
        };

        // Sending the message
        if write.send(my_ping_message).await.is_err() {
            error!("Failed to send ping");
            // handle error, maybe reconnect or notify
            break;
        }

        if let Err(e) = verify_pong(&mut read, &req_id).await {
            error!("Failed to verify pong: {:?}", e);
            // Handle verification failure (e.g., no pong received, wrong pong, etc.)
            // This could involve breaking the loop, logging the error, trying to reconnect, etc.
        }
    }
}

/// Verify pong response from the server to ensure connection health
async fn verify_pong(
    read: &mut mpsc::Receiver<MyMessage>,
    expected_req_id: &str,
) -> Result<(), String> {
    while let Some(my_message) = read.recv().await {
        if let Message::Text(json_text) = my_message.message {
            let v: Result<Value, _> = serde_json::from_str(&json_text);
            match v {
                Ok(data) => {
                    if data["op"] == "ping" && data["ret_msg"] == "pong" && data["req_id"] == expected_req_id {
                        debug!("Received pong message: {:?}", data);
                        // Now validate the pong message
                        if let Some(timestamp_str) = data["req_id"].as_str() {
                            debug!("Pong success {:?}", timestamp_str);
                            // ... further validation of the pong message ...
                        } else {
                            error!("Pong message does not contain timestamp");
                            return Err("No timestamp in pong message".to_string());
                        }
                    } else {
                        trace!("Received a non-pong message, skipping.");
                    }
                }
                Err(e) => {
                    error!("Failed to parse JSON: {}", e);
                }
            }
        } else {
            trace!("Received non-text message, skipping.");
        }
    }
    error!("Did not receive any more messages. Possibly disconnected.");
    Err("Failed to receive any pong messages".to_string())
}