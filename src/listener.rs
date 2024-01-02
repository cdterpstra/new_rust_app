use tokio::sync::broadcast;
use log::info;

use crate::BroadcastMessage;  // Ensure this is accessible in this module

/// Listens for messages from the broadcast channel and prints them
pub async fn listen_for_messages(mut receiver: broadcast::Receiver<BroadcastMessage>) {
    loop {
        match receiver.recv().await {
            Ok(broadcast_msg) => {
                // Print the received message to the screen
                info!("Received Message at {} from {}: {:?}", broadcast_msg.timestamp, broadcast_msg.endpoint_name, broadcast_msg.message);
            }
            Err(e) => {
                // Handle any errors (e.g., if the sender is dropped)
                info!("Error receiving broadcast message: {:?}", e);
                break;
            }
        }
    }
}
