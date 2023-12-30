// broadcast_task.rs

use tokio::sync::broadcast::{Sender as Broadcaster, error::SendError};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use log::{debug, warn}; // Import the debug! and warn! macros

pub async fn broadcast_task<S>(mut read_stream: S, broadcaster: Broadcaster<String>)
    where
        S: Stream<Item = String> + Unpin,
{
    debug!("Broadcast task started."); // Log when the task starts

    while let Some(message) = read_stream.next().await {
        debug!("Broadcasting message: {}", message);
        match broadcaster.send(message) {
            Ok(_) => debug!("Message broadcasted successfully."),
            Err(SendError(_)) => warn!("Error broadcasting message: Receiver might have dropped"),
        }
    }

    // If the loop exits (which might happen if the read_stream ends), log a message.
    warn!("Broadcast task stream ended, exiting task.");
}
