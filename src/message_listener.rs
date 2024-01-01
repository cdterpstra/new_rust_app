use tokio::sync::broadcast::Receiver;
use log::debug;
use crate::common::ConnectionMessage;

pub async fn listen_for_messages(mut receiver: Receiver<ConnectionMessage>) {
    loop {
        match receiver.recv().await {
            Ok(connection_message) => {
                // Assuming you want to log the message part of the ConnectionMessage
                if let Some(endpoint) = &connection_message.endpoint {
                    debug!("Received message for endpoint {}: {}", endpoint, connection_message.message);
                } else {
                    debug!("Received message with no endpoint: {}", connection_message.message);
                }
            }
            Err(e) => {
                debug!("Error receiving broadcast message: {:?}", e);
                break;
            }
        }
    }
}
