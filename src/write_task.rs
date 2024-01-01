// // write_task.rs
//
// use tokio::sync::mpsc::Receiver;
// use tokio_tungstenite::tungstenite::{Message, Error as WsError};
// use futures_util::{Sink, SinkExt};
// use std::fmt::Debug;
// use log::{debug, error, info};
// use tokio::time::{sleep, Duration};
//
// pub async fn write_task<W>(
//     mut write: W,
//     mut receiver: Receiver<String>,
// ) where
//     W: Sink<Message, Error = WsError> + Unpin,
//     W::Error: Debug, // Ensure the Error type implements Debug
// {
//     debug!("Write task started."); // Log when the task starts
//
//     loop {
//         match receiver.recv().await {
//             Some(message) => {
//                 debug!("Received message to send: {}", message); // Log received message
//                 let ws_message = Message::text(message);
//
//                 // Attempt to send the message and handle the result
//                 match write.send(ws_message).await {
//                     Ok(_) => {
//                         info!("Message sent successfully."); // Log successful send
//                     }
//                     Err(e) => {
//                         error!("Error sending message: {:?}", e); // Log errors
//                         // Decide on how to handle the error. For example, you might want to:
//                         // - Log the error and continue
//                         // - Attempt to reconnect or retry sending
//                         // - Exit the task if it's a critical unrecoverable error
//                         // For now, let's log and continue the loop.
//                     }
//                 }
//             },
//             None => {
//                 // When the receiver returns None, instead of ending, wait for a new message
//                 debug!("No more messages to send, waiting for new messages...");
//                 // Implementing a simple delay before checking for new messages
//                 // Adjust the duration based on how responsive you need the task to be
//                 sleep(Duration::from_secs(1)).await;
//                 // Consider implementing a reconnect or wakeup strategy if the channel is expected to close
//             }
//         }
//     }
// }
