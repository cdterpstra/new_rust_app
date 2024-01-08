// Import necessary crates and modules
use crate::websocket_manager::MyMessage;
use log::{debug, error, info};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

pub async fn start_subscribing(
    subscription_request_write: mpsc::Sender<MyMessage>,
    mut subscription_response_read: mpsc::Receiver<MyMessage>,
    uri: String,
) {
    let req_id = Uuid::new_v4().to_string();
    debug!("Generated req_id {} for next subscription message", req_id);

    let topics = vec!["tickers.BTCUSDT".to_string()];

    let public_topics_subscribe_message = json!({
        "req_id": req_id,
        "op": "subscribe",
        "args": topics
    })
    .to_string();

    debug!(
        "Constructed subscribe message: {}",
        public_topics_subscribe_message
    );

    // Constructing a text WebSocket message
    let subscribe_ws_message = Message::Text(public_topics_subscribe_message);

    // Wrapping it into your MyMessage structure
    let my_subscribe_message = MyMessage {
        timestamp: chrono::Utc::now().timestamp_millis() as u128,
        endpoint_name: uri.clone(),
        message: subscribe_ws_message,
    };

    // Sending the message
    if let Err(e) = subscription_request_write.send(my_subscribe_message).await {
        error!("Failed to send subscribe for uri {}: {:?}", uri, e);
        return; // Exiting function on send failure
    }

    // Verifying the subscription response
    match verify_subscription(&mut subscription_response_read, &req_id, &uri).await {
        Ok(_) => debug!(
            "Successfully verified subscription response for uri: {}",
            uri
        ),
        Err(e) => {
            error!("Failed to verify subscription for uri {}: {:?}", uri, e);
            // Depending on the application logic, you might want to return, retry, or ignore errors here
        }
    }

    info!("Subscription task ended for uri: {}", uri);
}

/// Verifies the subscription response from the server to ensure connection health
async fn verify_subscription(
    subscription_response_receiver: &mut mpsc::Receiver<MyMessage>,
    req_id: &str,
    uri: &str,
) -> Result<(), String> {
    // Listen for messages to verify the subscription
    while let Some(subscription_response) = subscription_response_receiver.recv().await {
        if let Message::Text(text) = subscription_response.message {
            match serde_json::from_str::<Value>(&text) {
                Ok(json) => {
                    debug!("Received subcribe message to verify: {}", text);
                    // Handle successful parsing
                    if json["req_id"] == req_id && json["op"] == "subscribe" {
                        // Check if subscription was successful here based on your application logic
                        info!(
                            "Valid subscription received for endpoint '{}' with req_id: {}",
                            uri, req_id
                        );
                        return Ok(());
                    } else {
                        // Continue or return an error if the subscription wasn't successful
                        continue;
                    }
                }
                Err(e) => {
                    // Log error if parsing fails
                    error!("Failed to parse incoming message as JSON: {:?}", e);
                    // Handle or return an error as appropriate
                }
            }
        }
    }
    Err("Failed to verify subscription or no more messages".to_string())
}
