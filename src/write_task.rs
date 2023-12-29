// write_task.rs

use tokio::sync::mpsc::Receiver;
use tokio_tungstenite::tungstenite::{Message, Error as WsError};
use std::sync::atomic::{AtomicBool, Ordering};
use futures_util::{Sink, SinkExt};
use std::fmt::Debug;
use std::sync::Arc;
use log::debug; // Import the debug! macro

pub async fn write_task<W>(
    mut write: W,
    mut receiver: Receiver<String>,
    is_connected: Arc<AtomicBool>,
) where
    W: Sink<Message, Error = WsError> + Unpin,
    W::Error: Debug, // Ensure the Error type implements Debug
{
    debug!("Write task started."); // Log when the task starts

    loop {
        match receiver.recv().await {
            Some(message) => {
                debug!("Received message to send: {}", message);

                if !is_connected.load(Ordering::SeqCst) {
                    debug!("Connection is no longer active. Write task terminating.");
                    break;
                }

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
            },
            None => {
                // When the receiver channel is closed, this block will execute.
                debug!("Receiver channel closed, write task terminating.");
                break;
            },
        }
    }

    debug!("Write task ended.");
}
