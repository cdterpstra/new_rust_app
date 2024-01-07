use std::env::args;
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
use colored;
use colored::Colorize;


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
            match serde_json::from_str::<Value>(&json_text) {
                Ok(data) => {
                    let op = data["op"].as_str().unwrap_or_default();

                    match op {
                        "ping" => {
                            // Handling for Spot, Inverse, and Linear endpoints
                            if data["ret_msg"] == "pong" && data["req_id"] == expected_req_id && data["success"].as_bool().unwrap_or(false) {
                                debug!("{} {}", "Pong success from Spot, Inverse, or Linear endpoint:".green(), data.to_string().green());
                            }
                        },
                        "pong" => {
                            // Determine if it's a Private or Option endpoint pong message
                            if data.get("req_id").map_or(false, |r| r == expected_req_id) {
                                // Handle Private endpoint pong message
                                if let Some(args) = data["args"].as_array() {
                                    if let Some(timestamp_str) = args.first().and_then(|v| v.as_str()) {
                                        if let Ok(timestamp) = timestamp_str.parse::<i128>() {
                                            if let Some(pong_time) = NaiveDateTime::from_timestamp_opt(
                                                (timestamp / 1000) as i64,
                                                ((timestamp % 1000) * 1_000_000) as u32,
                                            ).map(|naive| Utc.from_utc_datetime(&naive)) {
                                                let current_time = Utc::now();
                                                let duration_since_pong = current_time - pong_time;
                                                debug!("Time difference: Current Time: {} - Pong Time: {} = Duration (ms): {}", current_time, pong_time, duration_since_pong.num_milliseconds());

                                                if duration_since_pong <= chrono::Duration::milliseconds(200) {
                                                    debug!("{} {}", "Pong message is timely and within 200ms for Private endpoint:".green(), data.to_string().green());
                                                    return Ok(());
                                                } else {
                                                    error!("Pong message is too old. Connection might be unhealthy.");
                                                    return Err("Timestamp too old".to_string());
                                                }
                                            } else {
                                                error!("Failed to parse pong message's timestamp");
                                                return Err("Failed to parse timestamp".to_string());
                                            }
                                        } else {
                                            error!("Failed to parse pong message's timestamp");
                                            return Err("Failed to parse timestamp".to_string());
                                        }
                                    } else {
                                        error!("Pong message does not contain timestamp");
                                        return Err("No timestamp in pong message".to_string());
                                    }
                                }
                            } else {
                                // Handle Option endpoint pong message
                                if let Some(args) = data["args"].as_array() {
                                    if let Some(timestamp_str) = args.first().and_then(|v| v.as_str()) {
                                        if let Ok(timestamp) = timestamp_str.parse::<i128>() {
                                            if let Some(pong_time) = NaiveDateTime::from_timestamp_opt(
                                                (timestamp / 1000) as i64,
                                                ((timestamp % 1000) * 1_000_000) as u32,
                                            ).map(|naive| Utc.from_utc_datetime(&naive)) {
                                                let current_time = Utc::now();
                                                let duration_since_pong = current_time - pong_time;
                                                debug!("Time difference: Current Time: {} - Pong Time: {} = Duration (ms): {}", current_time, pong_time, duration_since_pong.num_milliseconds());

                                                if duration_since_pong <= chrono::Duration::milliseconds(200) {
                                                    debug!("{} {}", "Pong message is timely and within 200ms for Option endpoint:".green(), data.to_string().green());
                                                    return Ok(());
                                                } else {
                                                    error!("Pong message is too old. Connection might be unhealthy.");
                                                    return Err("Timestamp too old".to_string());
                                                }
                                            } else {
                                                error!("Failed to parse pong message's timestamp");
                                                return Err("Failed to parse timestamp".to_string());
                                            }
                                        } else {
                                            error!("Failed to parse pong message's timestamp");
                                            return Err("Failed to parse timestamp".to_string());
                                        }
                                    } else {
                                        error!("Pong message does not contain timestamp");
                                        return Err("No timestamp in pong message".to_string());
                                    }
                                }
                            }
                        },
                        _ => {
                            trace!("Received a non-pong or non-ping message, skipping.");
                        }
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
