// ping_manager.rs
use serde_json::Value;
use chrono::{NaiveDateTime, TimeZone, Utc};
use log::{debug, error, info, trace};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;
use crate::websocket_manager::MyMessage;
use colored::Colorize;

pub async fn start_pinging(write_ping: mpsc::Sender<MyMessage>, mut read_pong: mpsc::Receiver<MyMessage>, uri: String) {
    info!("Starting pinging task for uri: {}", uri);

    let mut interval = time::interval(Duration::from_secs(15));

    loop {
        interval.tick().await; // Wait for the next interval tick

        let req_id = Uuid::new_v4().to_string();
        debug!("Generated req_id {} for next ping message", req_id);

        let ping_message = json!({
            "op": "ping",
            "args": null,
            "req_id": req_id,
        })
            .to_string();

        debug!("Constructed ping message: {}", ping_message);

        // Constructing a text WebSocket message
        let ping_ws_message = Message::Text(ping_message);

        // Wrapping it into your MyMessage structure
        let my_ping_message = MyMessage {
            timestamp: chrono::Utc::now().timestamp_millis() as u128,
            endpoint_name: uri.clone(),
            message: ping_ws_message,
        };

        // Sending the message
        match write_ping.send(my_ping_message).await {
            Ok(_) => debug!("Ping message sent successfully for uri: {}", uri),
            Err(e) => {
                error!("Failed to send ping for uri {}: {:?}", uri, e);
                break; // Exiting loop on send failure
            }
        }

        // Verifying the pong response
        match verify_pong(&mut read_pong, &req_id).await {
            Ok(_) => debug!("Successfully verified pong response for uri: {}", uri),
            Err(e) => {
                error!("Failed to verify pong for uri {}: {:?}", uri, e);
                // Depending on the application logic, you might want to break, retry, or ignore errors here
            }
        }
    }

    info!("Pinging task ended for uri: {}", uri);
}

/// Verify pong response from the server to ensure connection health
async fn verify_pong(
    read: &mut mpsc::Receiver<MyMessage>,
    expected_req_id: &str,
) -> Result<(), String> {
    // Set the timeout duration
    let timeout_duration = Duration::from_secs(5);

    // Wait for a message or timeout
    let res = time::timeout(timeout_duration, read.recv()).await;

    match res {
        Ok(Some(my_message)) => {
            if let Message::Text(json_text) = my_message.message {
                let data = match serde_json::from_str::<Value>(&json_text) {
                    Ok(data) => data,
                    Err(e) => {
                        // Error parsing JSON; return an error immediately
                        error!("Failed to parse JSON: {}", e);
                        return Err("Failed to parse JSON".to_string());
                    }
                };

                let op = data["op"].as_str().unwrap_or_default();
                match op {
                    "ping" => return handle_spot_inverse_linear(&data, expected_req_id),
                    "pong" => return handle_pong(&data, expected_req_id),
                    _ => trace!("Received a non-pong or non-ping message, skipping."),
                }
            } else {
                trace!("Received non-text message, skipping.");
            }
        },
        Ok(None) => {
            // Stream is closed
            error!("Stream closed unexpectedly");
            return Err("Stream closed unexpectedly".to_string());
        },
        Err(_) => {
            // Timeout occurred
            error!("Timeout waiting for message");
            return Err("Timeout waiting for pong message".to_string());
        }
    }

    // If no valid message is processed, continue waiting for next message or return a generic error
    Err("No valid pong response received".to_string())
}

// Handle Spot, Inverse, and Linear endpoint pings
fn handle_spot_inverse_linear(data: &Value, expected_req_id: &str) -> Result<(), String> {
    if data["ret_msg"] == "pong" && data["req_id"] == expected_req_id && data["success"].as_bool().unwrap_or(false) {
        debug!("{} {}", "Pong success from Spot, Inverse, or Linear endpoint:".green(), data.to_string().green());
        Ok(())
    } else {
        Err("Unexpected message format for Spot/Inverse/Linear".to_string())
    }
}

// Handle Private or Option endpoint pongs
fn handle_pong(data: &Value, expected_req_id: &str) -> Result<(), String> {
    if data.get("req_id").map_or(false, |r| r == expected_req_id) {
        // Handling Private endpoint pong message
        return handle_private_pong(data);
    } else if data["op"] == "pong" {
        // Handling Option endpoint pong message
        return handle_option_pong(data);
    }
    Err("Failed to handle pong message".to_string())
}

// Function to handle the private endpoint pong messages
fn handle_private_pong(data: &Value) -> Result<(), String> {
    if let Some(args) = data["args"].as_array() {
        if let Some(timestamp_str) = args.first().and_then(|v| v.as_str()) {
            return check_pong_timestamp(timestamp_str, data);
        }
    }
    Err("Private pong message missing or invalid args".to_string())
}

// Function to handle the option endpoint pong messages
fn handle_option_pong(data: &Value) -> Result<(), String> {
    // Assuming that Option endpoint messages don't have a req_id and have a timestamp in the args
    if let Some(args) = data["args"].as_array() {
        if let Some(timestamp_str) = args.first().and_then(|v| v.as_str()) {
            return check_pong_timestamp(timestamp_str, data);
        }
    }
    Err("Option pong message missing or invalid args".to_string())
}

// Function to check the timestamp of a pong message
fn check_pong_timestamp(timestamp_str: &str, data: &Value) -> Result<(), String> {
    if let Ok(timestamp) = timestamp_str.parse::<i128>() {
        if let Some(pong_time) = NaiveDateTime::from_timestamp_opt(
            (timestamp / 1000) as i64,
            ((timestamp % 1000) * 1_000_000) as u32,
        ).map(|naive| Utc.from_utc_datetime(&naive)) {
            let current_time = Utc::now();
            let duration_since_pong = current_time - pong_time;
            debug!("Time difference: Current Time: {} - Pong Time: {} = Duration (ms): {}", current_time, pong_time, duration_since_pong.num_milliseconds());

            if duration_since_pong <= chrono::Duration::milliseconds(200) {
                debug!("{} {}", "Pong message is timely and within 200ms:".green(), data.to_string().green());
                Ok(())
            } else {
                error!("Pong message is too old. Connection might be unhealthy.");
                Err("Timestamp too old".to_string())
            }
        } else {
            error!("Failed to parse pong message's timestamp");
            Err("Failed to parse timestamp".to_string())
        }
    } else {
        error!("Failed to parse pong message's timestamp");
        Err("Failed to parse timestamp".to_string())
    }
}
