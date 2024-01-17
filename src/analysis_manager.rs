use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use log::info;
use serde_json::from_str;
use crate::websocket_manager::MyMessage;
use crate::write_to_database::MessageData;
use tungstenite::Message as WebSocketMessage;

/// Listens for messages from the broadcast channel and adds them to a hashmap
pub async fn listen_and_add_to_hashmap(mut receiver: broadcast::Receiver<MyMessage>) {
    let mut messages = HashMap::new();
    let retention_period_ns = 200 * 3600 * 1_000_000_000; // 200 hours in nanoseconds

    loop {
        match receiver.recv().await {
            Ok(my_msg) => {
                // Handle only text messages
                info!(
                    "Received Message with timestamp {} from {}: {:?}",
                    my_msg.receivedat, my_msg.endpoint_name, my_msg.message
                );

                if let WebSocketMessage::Text(ref text) = my_msg.message {
                    match from_str::<MessageData>(text) {
                        Ok(parsed_message) => {
                            // Opslaan van het bericht en de endpointnaam in de hashmap
                            messages.insert(
                                my_msg.receivedat,
                                (my_msg.endpoint_name.clone(), parsed_message.clone()),
                            );

                            // Verwijderen van oude berichten
                            let now_ns = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("Time went backwards")
                                .as_nanos() as i64;

                            messages.retain(|&key, _| now_ns - key < retention_period_ns);
                        },
                        Err(e) => {
                            info!("Failed to parse message: {}", e);
                        }
                    }
                }
            },
            Err(e) => {
                info!("Receiver error: {}", e);
                break;
            }
        }
    }
}