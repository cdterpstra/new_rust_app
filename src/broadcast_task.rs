use tokio::sync::broadcast::{Sender as Broadcaster};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use tokio::sync::mpsc::Sender as MpscSender;
use log::{error, warn};
use crate::common::ConnectionMessage;
use pin_utils::pin_mut; // Import the pin_mut macro
use tokio_tungstenite::tungstenite::Message;

pub async fn broadcast_task(
    identified_read: impl Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Send + 'static,
    broadcaster: Broadcaster<String>,
    work_sender: MpscSender<ConnectionMessage>,
    connection_id: u32,
) {
    pin_mut!(identified_read); // Pin the stream before using it

    while let Some(message_result) = identified_read.next().await {
        match message_result {
            Ok(Message::Text(text)) => {
                // Now you have a text message here
                let work = ConnectionMessage {
                    connection_id,
                    message: text.clone(),
                };

                // Send the work to the worker pool
                if let Err(e) = work_sender.send(work).await {
                    error!("Failed to dispatch incoming message to worker pool: {:#?}", e);
                }

                // Broadcast the message (optional depending on your use case)
                if let Err(e) = broadcaster.send(text) {
                    warn!("Error broadcasting message: {:#?}", e);
                }
            }
            Ok(_) => {
                // Handle other types of messages, or ignore them
            }
            Err(e) => {
                error!("Error reading message: {:?}", e);
                // Handle the error, possibly breaking out of the loop or logging it
            }
        }
    }
}