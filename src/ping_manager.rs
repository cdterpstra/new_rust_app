// Import necessary crates and modules
use crate::common::{BroadcastMessage, ManageTask, StartTaskMessage, Status};
use chrono::{Duration, TimeZone, Utc};
use log::{debug, error, info, trace};
use mockall::mock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Error;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::mpsc;
use tokio::{spawn, time};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

/// Represents a task for sending periodic ping messages

impl ManageTask {
    /// Starts the pinging process. This sends a ping message every 15 seconds
    /// and listens for pong messages to verify the connection status.
    async fn start_pinging(self) {
        let mut interval = time::interval(time::Duration::from_secs(15));
        tokio::time::sleep(time::Duration::from_secs(1)).await;
        loop {
            interval.tick().await;
            let req_id = Uuid::new_v4().to_string();
            let ping_message_json_string = json!({
                "op": "ping",
                "args": null,
                "req_id": req_id,
            })
            .to_string();

            debug!("Sending ping message: {}", ping_message_json_string);

            if let Err(e) = self
                .ws_sender
                .send(Message::Text(ping_message_json_string))
                .await
            {
                error!("Failed to send ping: {:?}", e);
                break;
            }

            spawn(verify_pong(
                self.broadcaster.resubscribe(),
                req_id.clone(),
                self.endpoint_name.clone(),
                self.status_sender.clone(),
            ));

            info!("Ping sent to {} with req_id {}", self.endpoint_name, req_id);
        }
    }
}

// Manager and Verification Functions Section

/// Manages incoming ping requests and spawns ping tasks accordingly
pub async fn ping_manager(
    mut ping_request_receiver: mpsc::Receiver<StartTaskMessage>,
    broadcaster: Receiver<BroadcastMessage>,
    internal_broadcaster: Sender<Status>,
) {
    while let Some(start_message) = ping_request_receiver.recv().await {
        let ping_task = ManageTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
            broadcaster: broadcaster.resubscribe(),
            status_sender: internal_broadcaster.clone(),
        };
        spawn(ping_task.start_pinging());
    }
}

/// Verifies the pong response from the server to ensure connection health
async fn verify_pong(
    mut broadcast_receiver: Receiver<BroadcastMessage>,
    expected_req_id: String,
    endpoint_name: String,
    status_sender: Sender<Status>,
) {
    while let Ok(broadcast_msg) = broadcast_receiver.recv().await {
        if let Message::Text(text) = &broadcast_msg.message {
            match serde_json::from_str::<Operation>(text) {
                Ok(operation) => {
                    match operation {
                        Operation::ping {
                            success,
                            ret_msg: _,
                            req_id,
                        } => {
                            if success && req_id == expected_req_id {
                                send_status(
                                    &status_sender,
                                    &endpoint_name,
                                    broadcast_msg.timestamp,
                                    "Ping/Pong: Connection healthy",
                                    "ping_manager",
                                )
                                .await;
                                debug!("Message: Connection healthy sent");
                            }
                        }
                        Operation::pong { args } => {
                            // Assuming timestamp is properly extracted from args and is a string representing u128
                            if let Some(timestamp_str) = args.first() {
                                debug!("Timestamp u128: {:?}", timestamp_str);
                                if let Ok(timestamp) = timestamp_str.parse::<i128>() {
                                    // Convert the u128 timestamp to DateTime<Utc>
                                    debug!("Timestamp datetime: {}", timestamp);
                                    if let Some(pong_time) =
                                        chrono::NaiveDateTime::from_timestamp_opt(
                                            (timestamp / 1000) as i64,               // converts milliseconds to seconds
                                            ((timestamp % 1000) * 1_000_000) as u32, // remainder as nanoseconds
                                        )
                                        .map(|naive| Utc.from_utc_datetime(&naive))
                                    {
                                        let current_time = Utc::now();
                                        let duration_since_pong = current_time - pong_time;
                                        debug!("Timestamp difference is curren_time: {} - pong_time {}", current_time, pong_time);
                                        debug!("Duration ms is: {}", duration_since_pong * 1000);
                                        if duration_since_pong <= Duration::milliseconds(200) {
                                            // Timestamp is within the last 5 seconds
                                            send_status(
                                                &status_sender,
                                                &endpoint_name,
                                                broadcast_msg.timestamp,
                                                "Ping/Pong: Connection healthy",
                                                "ping_manager",
                                            )
                                            .await;
                                            debug!("Message: Connection healthy sent");
                                        } else {
                                            // Timestamp is older than 5 seconds
                                            send_status(
                                                &status_sender,
                                                &endpoint_name,
                                                broadcast_msg.timestamp,
                                                "Ping/Pong: Timestamp older than 5 seconds",
                                                "ping_manager",
                                            )
                                            .await;
                                            debug!("Message: Timestamp older than 5 seconds sent");
                                        }
                                    } else {
                                        // Handle case where timestamp is invalid or pong message doesn't fit expected structure
                                        error!("Failed to parse pong message for timestamp validation or timestamp is out of range");
                                    }
                                } else {
                                    // Handle case where timestamp string could not be parsed to a u128
                                    error!("Failed to parse timestamp string to u128");
                                }
                            } else {
                                // Handle case where args does not contain any elements or timestamp
                                error!("Args does not contain timestamp or is empty");
                            }
                        }
                    }
                }
                Err(_e) => {
                    trace!("Received a non-targeted message, skipping.");
                }
            }
        }
    }
}

async fn send_status(
    status_sender: &Sender<Status>,
    endpoint_name: &String,
    timestamp: u128,
    message: &str,
    sending_party: &str,
) {
    let status_message = Status {
        endpoint_name: endpoint_name.clone(),
        timestamp,
        message: message.to_string(),
        sending_party: sending_party.to_string(),
    };

    debug!("Attempting to send status message: {:?}", status_message);

    if let Err(e) = status_sender.send(status_message) {
        error!("Failed to send status message: {:?}", e);
    } else {
        debug!(
            "Status message sent successfully for endpoint '{}'",
            endpoint_name
        );
    }
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "op")]
enum Operation {
    ping {
        success: bool,
        ret_msg: String,
        req_id: String,
    },
    pong {
        args: Vec<String>,
    },
}

// Define the trait if not already defined
pub trait MessageSender {
    async fn send(&self, msg: Message) -> Result<(), Error>;
}

mock! {
    Sender {}

    // #[async_trait]
    impl MessageSender<> for Sender {
        async fn send(&self, msg: Message) -> Result<(), Error>;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::env;
    use std::time::Instant;
    use tokio::sync::{broadcast, mpsc};

    #[tokio::test]
    async fn test_initial_ping_message() {
        // env::set_var("RUST_LOG", "trace");
        let _ = env_logger::builder().is_test(true).try_init();

        debug!("Setting up test.");
        // Arrange
        let (tx, mut rx) = mpsc::channel(1);
        let (status_tx, _) = broadcast::channel(5);
        let (broadcaster_tx, _) = broadcast::channel(1);

        let ping_task = ManageTask {
            endpoint_name: "test_endpoint".to_string(),
            ws_sender: tx,
            broadcaster: broadcaster_tx.subscribe(),
            status_sender: status_tx,
        };

        // Act
        spawn(async move {
            ping_task.start_pinging().await;
        });

        // Assert
        if let Some(msg) = rx.recv().await {
            if let Message::Text(text) = msg {
                // Parse the text to a JSON Value
                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                    // Verify that the structure matches the expected format
                    let is_correct_op = parsed.get("op").map_or(false, |op| op == "ping");
                    let is_args_null = parsed.get("args").map_or(false, |args| args.is_null());
                    let is_req_id_valid = if let Some(req_id) = parsed.get("req_id") {
                        // Check if req_id is a valid UUID format
                        Uuid::parse_str(req_id.as_str().unwrap_or("")).is_ok()
                    } else {
                        false
                    };

                    // Assert that all conditions are true
                    assert!(is_correct_op, "The 'op' field must be 'ping'");
                    assert!(is_args_null, "The 'args' field must be null");
                    assert!(is_req_id_valid, "The 'req_id' field must be a valid UUID");
                } else {
                    panic!("Failed to parse message text to JSON");
                }
            } else {
                panic!("Received a non-text message when a text message was expected");
            }
        } else {
            panic!("No message received");
        }
    }

    #[tokio::test]
    async fn test_verify_time_between_ping() {
        // env::set_var("RUST_LOG", "debug");
        let _ = env_logger::builder().is_test(true).try_init();

        debug!("Setting up test.");
        // Arrange
        let (tx, mut rx) = mpsc::channel(1);
        let (status_tx, mut status_rx) = broadcast::channel(5);
        let (broadcaster_tx, _) = broadcast::channel(1);

        let ping_task = ManageTask {
            endpoint_name: "test_endpoint".to_string(),
            ws_sender: tx,
            broadcaster: broadcaster_tx.subscribe(),
            status_sender: status_tx,
        };
        let start = Instant::now();
        // Act
        spawn(async move {
            ping_task.start_pinging().await;
        });

        // Assert
        if let Some(msg) = rx.recv().await {
            // Check the elapsed time
            assert!(matches!(msg, Message::Text(ref text) if text.contains("ping")));
            let elapsed = start.elapsed();
            assert!(
                elapsed.as_millis() > 1000 && elapsed.as_millis() < 1005,
                "More then 1000 milliseconds and Less than 1005 milliseconds to send first message"
            );
            debug!("Time elapsed first ping: {:?}", elapsed.as_millis());
        } else {
            panic!("No message was sent!");
        }
        if let Some(msg) = rx.recv().await {
            assert!(matches!(msg, Message::Text(ref text) if text.contains("ping")));
            // Check the elapsed time
            let elapsed = start.elapsed();
            assert!(
                elapsed.as_secs() > 14 && elapsed.as_secs() < 20,
                "More then 15 seconds and less than 20 seconds elapsed between messages"
            );
            debug!("Time elapsed second ping: {:?}", elapsed.as_millis());
        } else {
            panic!("No message was sent!");
        }

        // Simulate sending a pong message back through the broadcaster
        let simulated_pong = json!({
            "op": "pong",
            "args": [Utc::now().timestamp_millis().to_string()] // Simulate current timestamp
        })
        .to_string();

        debug!("pong response: {:?}", simulated_pong);

        let broadcast_msg = BroadcastMessage {
            message: Message::Text(simulated_pong.clone()), // Assuming you've corrected the alias or type as previously discussed
            timestamp: Utc::now().timestamp_millis() as u128,
            endpoint_name: "test_endpoint".to_string(),
        };

        // Log the message before sending
        debug!("Broadcasting pong message: {:?}", broadcast_msg);

        // Send the message
        broadcaster_tx.send(broadcast_msg).unwrap();

        // Assert: Receive and check the status message
        match status_rx.recv().await {
            Ok(received_msg) => {
                assert!(
                    received_msg.message.contains("healthy"),
                    "The status message does not indicate a healthy connection."
                );
                debug!(
                    "Received status message indicating a healthy connection: {:?}",
                    received_msg
                );
            }
            Err(e) => panic!("Failed to receive status message: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_verify_option_pong_received() {
        let _ = env_logger::builder().is_test(true).try_init();

        debug!("Setting up test.");
        // Arrange
        let (tx, mut rx) = mpsc::channel(1);
        let (status_tx, mut status_rx) = broadcast::channel(5);
        let (broadcaster_tx, _) = broadcast::channel(1);

        let ping_task = ManageTask {
            endpoint_name: "test_endpoint".to_string(),
            ws_sender: tx,
            broadcaster: broadcaster_tx.subscribe(),
            status_sender: status_tx,
        };

        // Act
        spawn(async move {
            ping_task.start_pinging().await;
        });

        // Assert
        if let Some(msg) = rx.recv().await {
            // Check the elapsed time
            assert!(matches!(msg, Message::Text(ref text) if text.contains("ping")));
        } else {
            panic!("No message was sent!");
        }

        // Simulate sending a pong message back through the broadcaster
        let simulated_pong = json!({
            "op": "pong",
            "args": [Utc::now().timestamp_millis().to_string()] // Simulate current timestamp
        })
        .to_string();

        debug!("pong response: {:?}", simulated_pong);
        let timestamp = Utc::now().timestamp_millis() as u128;
        let broadcast_msg = BroadcastMessage {
            message: Message::Text(simulated_pong.clone()),
            timestamp,
            endpoint_name: "test_endpoint".to_string(),
        };

        // Log the message before sending
        debug!("Broadcasting pong message: {:?}", broadcast_msg);

        // Send the message
        broadcaster_tx.send(broadcast_msg).unwrap();

        // Assert: Receive and check the status message
        match status_rx.recv().await {
            Ok(received_msg) => {
                // Assuming you have expected values for these variables
                let expected_endpoint_name = "test_endpoint".to_string(); // replace with actual expected value
                let expected_timestamp = timestamp; // replace with actual expected value or a reasonable range
                let expected_message = "Ping/Pong: Connection healthy".to_string(); // replace with actual expected value
                let expected_sending_party = "ping_manager".to_string(); // replace with actual expected value

                assert_eq!(
                    received_msg.endpoint_name, expected_endpoint_name,
                    "The endpoint name of the status message does not match."
                );
                // Note: For timestamp you might want to assert a range if it's based on the current time
                assert_eq!(
                    received_msg.timestamp, expected_timestamp,
                    "The timestamp of the status message does not match."
                );
                assert_eq!(
                    received_msg.message, expected_message,
                    "The content of the status message does not match."
                );
                assert_eq!(
                    received_msg.sending_party, expected_sending_party,
                    "The sending party of the status message does not match."
                );

                debug!("Received status message as expected: {:?}", received_msg);
            }
            Err(e) => panic!("Failed to receive status message: {:?}", e),
        }
    }

    #[tokio::test]
    #[allow(unused_assignments)]
    async fn test_verify_spot_linear_inverse_pong_received() {
        env::set_var("RUST_LOG", "trace");
        let _ = env_logger::builder().is_test(true).try_init();

        debug!("Setting up test.");
        // Arrange
        let (tx, mut rx) = mpsc::channel(1);
        let (status_tx, mut status_rx) = broadcast::channel(5);
        let (broadcaster_tx, _) = broadcast::channel(1);

        let ping_task = ManageTask {
            endpoint_name: "test_endpoint".to_string(),
            ws_sender: tx,
            broadcaster: broadcaster_tx.subscribe(),
            status_sender: status_tx,
        };

        // Act
        spawn(async move {
            ping_task.start_pinging().await;
        });

        let mut captured_req_id = String::new();

        // Assert and parse UUID
        if let Some(msg) = rx.recv().await {
            if let Message::Text(text) = msg {
                if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                    if let Some(req_id) = parsed["req_id"].as_str() {
                        // Capture the req_id for later use
                        captured_req_id = req_id.to_string();
                        // Check if it's a ping message and contains the correct req_id
                        assert!(text.contains("ping"), "Message does not contain 'ping'");
                    } else {
                        panic!("req_id not found or not a string");
                    }
                } else {
                    panic!("Failed to parse message text to JSON");
                }
            } else {
                panic!("Received a non-text message when a text message was expected");
            }
        } else {
            panic!("No message was received!");
        }

        // Simulate sending a pong message back through the broadcaster
        let simulated_pong = json!({
        "success": true,
        "ret_msg": "pong",
        "conn_id": "0970e817-426e-429a-a679-ff7f55e0b16a",
        "op": "ping",
        "req_id": captured_req_id,
        })
        .to_string();

        debug!("pong response: {:?}", simulated_pong);
        let timestamp = Utc::now().timestamp_millis() as u128;
        let broadcast_msg = BroadcastMessage {
            message: Message::Text(simulated_pong.clone()),
            timestamp,
            endpoint_name: "test_endpoint".to_string(),
        };

        // Log the message before sending
        debug!("Broadcasting pong message: {:?}", broadcast_msg);

        // Send the message
        broadcaster_tx.send(broadcast_msg).unwrap();

        // Assert: Receive and check the status message
        match status_rx.recv().await {
            Ok(received_msg) => {
                // Assuming you have expected values for these variables
                let expected_endpoint_name = "test_endpoint".to_string(); // replace with actual expected value
                let expected_timestamp = timestamp; // replace with actual expected value or a reasonable range
                let expected_message = "Ping/Pong: Connection healthy".to_string(); // replace with actual expected value
                let expected_sending_party = "ping_manager".to_string(); // replace with actual expected value

                assert_eq!(
                    received_msg.endpoint_name, expected_endpoint_name,
                    "The endpoint name of the status message does not match."
                );
                // Note: For timestamp you might want to assert a range if it's based on the current time
                assert_eq!(
                    received_msg.timestamp, expected_timestamp,
                    "The timestamp of the status message does not match."
                );
                assert_eq!(
                    received_msg.message, expected_message,
                    "The content of the status message does not match."
                );
                assert_eq!(
                    received_msg.sending_party, expected_sending_party,
                    "The sending party of the status message does not match."
                );

                debug!("Received status message as expected: {:?}", received_msg);
            }
            Err(e) => panic!("Failed to receive status message: {:?}", e),
        }
    }
}
