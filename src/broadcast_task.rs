use tokio::sync::broadcast::{Sender as Broadcaster};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use tokio::sync::mpsc::Sender as MpscSender;
use tokio::time::{self, Duration};
use log::{error, warn, info};
use crate::common::ConnectionMessage;
use pin_utils::pin_mut; // Import the pin_mut macro
use tokio_tungstenite::tungstenite::Message;

pub async fn broadcast_task(
    identified_read: impl Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Send + 'static,
    broadcaster: Broadcaster<String>,
    work_sender: MpscSender<ConnectionMessage>,
    endpoint: String,
    connection_timestamp: u128,
) {
    pin_mut!(identified_read); // Pin the stream before using it

    while let Ok(Some(message_result)) = time::timeout(Duration::from_secs(2000), identified_read.next()).await {
        match message_result {
            Ok(Message::Text(text)) => {
                let work = ConnectionMessage {
                    endpoint: endpoint.clone(),
                    connection_timestamp,
                    message: text.clone(),
                };

                if let Err(e) = work_sender.send(work).await {
                    error!("Failed to dispatch incoming message to worker pool: {:#?}", e);
                }

                if let Err(e) = broadcaster.send(text) {
                    warn!("Error broadcasting message: {:#?}", e);
                }
            }
            Ok(_) => {
                // Handle other message types or ignore them
            }
            Err(e) => {
                error!("Error reading message: {:?}", e);
                break;  // Break if there's an error reading message
            }
        }
    }

    // Either the timeout was reached, or an error occurred
    if identified_read.next().await.is_none() {
        warn!("Message stream ended, no further messages will be received.");
    } else {
        warn!("No messages received for 20 seconds, stopping broadcast task.");
    }

    // Perform any necessary cleanup before exiting
    info!("Broadcast task has ended.");
}
