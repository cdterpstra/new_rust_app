// Import necessary crates and modules
use crate::common::{BroadcastMessage, StartPingMessage, Status};
use chrono::{Duration, TimeZone, Utc};
use log::{debug, error, info, trace};
use mockall::mock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Error;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::{mpsc};
use tokio::{spawn, time};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

// Structs and Implementations Section

/// Represents a task for sending periodic ping messages
#[derive(Debug)]
struct PingTask {
    endpoint_name: String,
    ws_sender: mpsc::Sender<Message>,
    broadcaster: Receiver<BroadcastMessage>,
    status_sender: Sender<Status>,
}

impl PingTask {
    /// Starts the pinging process. This sends a ping message every 15 seconds
    /// and listens for pong messages to verify the connection status.
    async fn start_pinging(self) {
        let mut interval = time::interval(time::Duration::from_secs(15));
        tokio::time::sleep(core::time::Duration::from_secs(1)).await;
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
    mut ping_request_receiver: mpsc::Receiver<StartPingMessage>,
    broadcaster: Receiver<BroadcastMessage>,
    internalbroadcaster: Sender<Status>,
) {
    while let Some(start_message) = ping_request_receiver.recv().await {
        let ping_task = PingTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
            broadcaster: broadcaster.resubscribe(),
            status_sender: internalbroadcaster.clone(),
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
                                    "pingmanager",
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
                                                "pingmanager",
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
                                                "pingmanager",
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
    use std::time::Instant;
    use super::*;
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

        let ping_task = PingTask {
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
            // Inspect the message to ensure it's a ping with the right format
            assert!(matches!(msg, Message::Text(ref text) if text.contains("ping")));
            debug!("Received 1st ping message: {:?}", msg);
        } else {
            panic!("No message was sent!");
        }
    }

    #[tokio::test]
    async fn test_verify_ping_message() {
        // env::set_var("RUST_LOG", "debug");
        let _ = env_logger::builder().is_test(true).try_init();

        debug!("Setting up test.");
        // Arrange
        let (tx, mut rx) = mpsc::channel(1);
        let (status_tx, mut status_rx) = broadcast::channel(5);
        let (broadcaster_tx, _) = broadcast::channel(1);

        let ping_task = PingTask {
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
            // Inspect the message to ensure it's a ping with the right format
            assert!(matches!(msg, Message::Text(ref text) if text.contains("ping")));
            debug!("Received 1st ping message: {:?}", msg);
        } else {
            panic!("No message was sent!");
        }
        if let Some(msg) = rx.recv().await {
            assert!(matches!(msg, Message::Text(ref text) if text.contains("ping")));
            // Check the elapsed time
            let elapsed = start.elapsed();
            assert!(elapsed.as_secs() < 20, "More than 20 seconds elapsed between messages");
            debug!("Received 2nd message: {:?}", msg);
            // Assertions...
        } else {
            error!("No message was sent!");
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
        // Wait and receive the status message sent by verify_pong
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
}
