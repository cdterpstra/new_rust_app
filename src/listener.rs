use tokio::sync::broadcast;
use log::{debug, trace};  // Make sure to include this if you're using the `info!` macro

// Assuming MyMessage is defined in the `crate::websocket_manager` module
use crate::websocket_manager::MyMessage;

/// Listens for messages from the broadcast channel and prints them
pub async fn listen_for_messages(mut receiver: broadcast::Receiver<MyMessage>) {
    loop {
        match receiver.recv().await {
            Ok(my_msg) => {  // Changed from Ok() to Some() as recv() returns Option
            // Print the received message to the screen
            trace!(
                    "Received Message with timestamp {} from {}: {:?}",
                    my_msg.receivedat, my_msg.endpoint_name, my_msg.message
                );
            }
            Err(e) => {  // Changed from Err(e) to None as recv() returns Option
            // Handle any errors (e.g., if the sender is dropped)
            debug!("Sender has been dropped or channel is closed {}", e);
                break;
            }
        }
    }
}
