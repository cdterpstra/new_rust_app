use tokio::sync::mpsc;
use log::info;  // Make sure to include this if you're using the `info!` macro

// Assuming MyMessage is defined in the `crate::websocket_manager` module
use crate::websocket_manager::MyMessage;

/// Listens for messages from the broadcast channel and prints them
pub async fn listen_for_messages(mut receiver: mpsc::Receiver<MyMessage>) {
    loop {
        match receiver.recv().await {
            Some(my_msg) => {  // Changed from Ok() to Some() as recv() returns Option
            // Print the received message to the screen
            info!(
                    "Received Message with timestamp {} from {}: {:?}",
                    my_msg.timestamp, my_msg.endpoint_name, my_msg.message
                );
            }
            None => {  // Changed from Err(e) to None as recv() returns Option
            // Handle any errors (e.g., if the sender is dropped)
            info!("Sender has been dropped or channel is closed");
                break;
            }
        }
    }
}
