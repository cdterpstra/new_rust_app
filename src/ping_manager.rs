// Import necessary crates and modules
use crate::common::{BroadcastMessage, StartPingMessage, Status};
use log::{debug, error, info};
use serde_json::{json, Value};
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::mpsc;
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
            match serde_json::from_str::<Value>(text) {
                Ok(json) => {
                    // ... handle successful parsing ...
                    if json["op"] == "ping" && json["req_id"] == expected_req_id && json["success"] == true {
                        info!("Valid pong received for endpoint '{}' with req_id: {}", endpoint_name, expected_req_id);

                        let pong_message = Status {
                            endpoint_name: endpoint_name.clone(),
                            timestamp: broadcast_msg.timestamp,
                            message: "Ping/Pong: Connection healthy".to_string(),
                            sending_party: "pingmanager".to_string(),
                        };

                        debug!("Attempting to send PongStatus message: {:?}", pong_message);

                        if let Err(e) = status_sender.send(pong_message) {
                            error!("Ping/Pong: Connection unhealthy {:?}", e);
                        } else {
                            debug!("PongStatus message sent successfully for endpoint '{}'", endpoint_name);
                        }
                    }
                }
                Err(e) => {
                    // Now `e` is in scope and holds the error
                    error!("Failed to parse incoming message as JSON: {:?}", e);
                }
            }
        }
    }
}
