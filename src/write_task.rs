// write_task.rs

use tokio::sync::mpsc::Receiver;
use tokio_tungstenite::tungstenite::{Message, Error as WsError};
use futures_util::{Sink, SinkExt};
use std::fmt::Debug;
use log::debug; // Import the debug! macro

pub async fn write_task<W>(
    mut write: W,
    mut receiver: Receiver<String>,
) where
    W: Sink<Message, Error = WsError> + Unpin,
    W::Error: Debug, // Ensure the Error type implements Debug
{
    debug!("Write task started."); // Log when the task starts

    while let Some(message) = receiver.recv().await {
        debug!("Received message to send: {}", message);

        let ws_message = Message::text(message);

        match write.send(ws_message).await {
            Ok(_) => {
                debug!("Message sent successfully.");
            }
            Err(e) => {
                debug!("Error sending message: {:?}", e);
                // Decide on how to handle the error. For example, you might want to attempt to reconnect or just log the error.
                break;
            }
        }
    }

    debug!("Write task ended.");
}
