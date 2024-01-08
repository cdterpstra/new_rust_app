use url::Url;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use colored::Colorize;
use config::{Config, File};
use futures_util::StreamExt;
// Import necessary crates and modules
use crate::websocket_manager::MyMessage;
use log::{debug, error, info};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use crate::config::AppConfig;
use tokio::time::{sleep, timeout};
// use tokio_tungstenite::tungstenite::Error::Url;


pub async fn start_subscribing(
    subscription_request_write: mpsc::Sender<MyMessage>,
    mut subscription_response_read: mpsc::Receiver<MyMessage>,
    uri: String,
) -> Result<(), Box<dyn Error>> {

    // Parse the URI to get the last segment
    let parsed_uri = Url::parse(&uri)?;
    let segments: Vec<&str> = parsed_uri.path_segments().map(|c| c.collect()).unwrap_or_default();
    let last_segment = segments.last().ok_or("Invalid URI: Cannot determine last segment")?;

    // Set up configuration
    let settings = Config::builder()
        .add_source(File::with_name("config/default"))
        .build()?;

    let mut app_config: AppConfig = settings.try_deserialize()?;

    // Removing duplicates from trading_pairs
    app_config.trading_pairs = app_config.trading_pairs.iter().cloned().collect::<HashSet<_>>().into_iter().collect();

    // Dynamically select topics based on the last part of the URI
    let topics_to_subscribe = app_config.topics.get(*last_segment).ok_or(format!("No topics found for {}", last_segment))?;

    // Ensure no duplicate topics
    let unique_topics: Vec<String> = topics_to_subscribe.iter().cloned().collect::<HashSet<_>>().into_iter().collect();

    // Variables for clarity and modification
    let all_topics = unique_topics.iter()
        .flat_map(|topic| app_config.trading_pairs.iter().map(move |pair| format!("{}.{}", topic, pair)))
        .collect::<Vec<_>>();

    // Splitting into chunks of 10 and subscribing to each chunk
    for chunk in all_topics.chunks(10) {
        let req_id = Uuid::new_v4().to_string();
        debug!("Generated req_id {} for next subscription message", req_id);

        let topics: Vec<_> = chunk.iter().collect();

        let public_topics_subscribe_message = json!({
            "req_id": req_id,
            "op": "subscribe",
            "args": topics
        })
            .to_string();

        debug!(
            "Constructed subscribe message for chunk: {}",
            public_topics_subscribe_message
        );

        let subscribe_ws_message = Message::Text(public_topics_subscribe_message);

        let my_subscribe_message = MyMessage {
            timestamp: chrono::Utc::now().timestamp_millis() as u128,
            endpoint_name: uri.clone(),
            message: subscribe_ws_message,
        };

        // Sending the message for the chunk
        if let Err(e) = subscription_request_write.send(my_subscribe_message).await {
            error!("Failed to send subscribe for uri {}: {:?}", uri, e);
            return Err(Box::new(e)); // Exiting function on send failure
        }

        // Verifying the subscription response for each chunk
        match verify_subscription(&mut subscription_response_read, &req_id, &uri).await {
            Ok(_) => debug!(
                "Successfully verified subscription response for chunk in uri: {}",
                uri.green()
            ),
            Err(e) => {
                error!("Failed to verify subscription for uri {}: {:?}", uri, e);
                // Logic for handling failure, possibly continue to the next chunk or handle error
            }
        }

        // Pause for 2 seconds before next subscription
        sleep(core::time::Duration::from_millis(50)).await;
    }

    info!("Subscription task ended for uri: {}", uri);

    Ok(())
}



/// Verifies the subscription response from the server to ensure connection health
async fn verify_subscription(
    subscription_response_receiver: &mut mpsc::Receiver<MyMessage>,
    req_id: &str,
    uri: &str,
) -> Result<(), String> {
    // Set a 2-second timeout for receiving messages
    if let Ok(Some(subscription_response)) = timeout(core::time::Duration::from_secs(2), subscription_response_receiver.recv()).await {
        if let Message::Text(text) = subscription_response.message {
            debug!("Attempting to verify message: {}", text); // Additional log
            match serde_json::from_str::<Value>(&text) {
                Ok(json) => {
                    // Additional diagnostics:
                    debug!("Received JSON for verification: req_id: {}, op: {}", json["req_id"], json["op"]);

                    if json["req_id"] == req_id && json["op"] == "subscribe" && json["success"] == true {
                        info!(
                            "Valid subscription received for endpoint '{}' with req_id: {}",
                            uri, req_id
                        );
                        return Ok(());
                    } else {
                        // Log why it's continuing, if the message isn't a match
                        debug!("Continuing verification: Message didn't match the expected subscription confirmation.");
                    }
                }
                Err(e) => {
                    error!("Failed to parse incoming message as JSON: {:?}", e);
                    return Err(format!("Failed to parse incoming message as JSON: {:?}", e)); // Propagating error
                }
            }
        }
    } else {
        // Timeout or other error occurred
        return Err("No message received in 2 seconds for verification".to_string());
    }
    // If the loop exits without returning, it means we've run out of messages without verifying
    Err("Failed to verify subscription or no more messages".to_string())
}
