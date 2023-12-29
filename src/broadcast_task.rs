// broadcast_task.rs

use tokio::sync::broadcast::{Sender as Broadcaster, error::SendError};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use crate::types::MessageWithWSID;
use log::{debug, warn}; // Import the debug! and warn! macros

pub async fn broadcast_task<S>(mut read_stream: S, broadcaster: Broadcaster<MessageWithWSID>)
    where
        S: Stream<Item = MessageWithWSID> + Unpin,
{
    debug!("Broadcast task started."); // Log when the task starts

    loop {
        // Instead of exiting on None, we just wait for the next message forever.
        match read_stream.next().await {
            Some(message) => {
                debug!("Broadcasting message: {:?}", message);
                match broadcaster.send(message) {
                    Ok(_) => debug!("Message broadcasted successfully."),
                    Err(SendError(_)) => warn!("Error broadcasting message: Receiver might have dropped"),
                }
            },
            None => {
                // This block will execute if the stream becomes empty. It shouldn't in a well-behaved WebSocket connection.
                // Consider adding reconnection logic here if needed.
                warn!("Read stream returned None, waiting for new messages...");
                // Instead of breaking, continue to stay in the loop and wait for new messages.
                // You might want to include a delay here to prevent busy waiting.
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            },
        }
    }
}
