// Import necessary crates and modules
use crate::common::{BroadcastMessage, Status, SubscriptionMessage};
use log::{debug, error, info};
use serde_json::{json, Value};
use tokio::spawn;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

// ===================================
// == Struct Definition Section ==
// ===================================

/// A task responsible for managing subscriptions
#[derive(Debug)]
struct SubscriptionTask {
    endpoint_name: String,
    ws_sender: mpsc::Sender<Message>,
    broadcaster: Receiver<BroadcastMessage>,
    subscription_status_sender: Sender<Status>,
}

// ===================================
// == Implementations Section ==
// ===================================

impl SubscriptionTask {
    /// Starts the process of subscribing to a specific topic and verifying the subscription
    async fn start_subscribing(self) {
        // Construct subscription message with unique request ID
        let req_id = Uuid::new_v4().to_string();
        let subscription_message_json_string = json!({
            "req_id": req_id,
            "op": "subscribe",
            "args": ["tickers.BTCUSDT"]
        }).to_string();

        debug!("Sending subscription message: {}", subscription_message_json_string);

        // Attempt to send the subscription message
        if let Err(e) = self.ws_sender.send(Message::Text(subscription_message_json_string)).await {
            error!("Failed to send subscription: {:?}", e);
            return; // Return early if we can't send the subscription
        }

        // Start verification process
        if let Err(e) = spawn(verify_subscription(
            self.broadcaster.resubscribe(),
            req_id.clone(),
            self.endpoint_name.clone(),
            self.subscription_status_sender.clone(),
        )).await {
            error!("Verification task failed: {:?}", e);
        }

        info!("Subscription sent to {} with req_id {}", self.endpoint_name, req_id);
    }
}

// ===================================
// == Async Functions Section ==
// ===================================

/// Manages incoming subscription requests and spawns tasks to handle them
pub async fn subscription_manager(
    mut subscription_request_receiver: mpsc::Receiver<SubscriptionMessage>,
    broadcaster: Receiver<BroadcastMessage>,
    internalbroadcaster: Sender<Status>,
) {
    while let Some(start_message) = subscription_request_receiver.recv().await {
        // Construct and spawn a new subscription task for each request
        let subscription_task = SubscriptionTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
            broadcaster: broadcaster.resubscribe(),
            subscription_status_sender: internalbroadcaster.clone(),
        };
        spawn(subscription_task.start_subscribing());
    }
}

/// Verifies the subscription response from the server to ensure connection health
async fn verify_subscription(
    mut broadcast_receiver: Receiver<BroadcastMessage>,
    expected_req_id: String,
    endpoint_name: String,
    status_sender: Sender<Status>,
) {
    // Listen for broadcast messages to verify the subscription
    while let Ok(broadcast_msg) = broadcast_receiver.recv().await {
        if let Message::Text(text) = &broadcast_msg.message {
            match serde_json::from_str::<Value>(text) {
                Ok(json) => {
                    // Handle successful parsing
                    if json["op"] == "subscribe" && json["req_id"] == expected_req_id && json["success"] == true {
                        info!("Valid subscription received for endpoint '{}' with req_id: {}", endpoint_name, expected_req_id);

                        // Construct and send a status message indicating successful subscription
                        let subscription_message = Status {
                            endpoint_name: endpoint_name.clone(),
                            timestamp: broadcast_msg.timestamp,
                            message: "Subscription: Subscription Successful".to_string(),
                            sending_party: "subscription_manager".to_string(),
                        };
                        debug!("Attempting to send SubscriptionStatus message: {:?}", subscription_message);
                        if let Err(e) = status_sender.send(subscription_message) {
                            error!("Subscription: Subscription Unsuccessful {:?}", e);
                        } else {
                            debug!("Subscription message sent successfully for endpoint '{}'", endpoint_name);
                        }
                    }
                }
                Err(e) => {
                    // Log error if parsing fails
                    error!("Failed to parse incoming message as JSON: {:?}", e);
                }
            }
        }
    }
}
