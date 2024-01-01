// use std::sync::Arc;
// use tokio::sync::broadcast::{Sender as Broadcaster};
// use futures_util::stream::Stream;
// use futures_util::StreamExt;
// use tokio::time::{self, Duration, Instant};
// use log::{error, warn, info, debug}; // Ensure debug is included for logging
// use crate::common::ConnectionMessage;
// use pin_utils::pin_mut;
// use tokio::sync::Mutex;
// // Import the pin_mut macro
// use tokio_tungstenite::tungstenite::Message;
//
// pub async fn broadcast_task(
//     identified_read: impl Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Send + 'static,
//     broadcaster: Broadcaster<ConnectionMessage>,
//     endpoint: String,
//     connection_timestamp: u128,
// ) {
//     pin_mut!(identified_read); // Pin the stream before using it
//
//     let last_message_time = Arc::new(Mutex::new(Instant::now()));
//     let last_message_time_clone = Arc::clone(&last_message_time);
//
//     let endpoint_clone = endpoint.clone(); // Clone the endpoint for the spawned task
//     tokio::spawn(async move {
//         loop {
//             time::sleep(Duration::from_secs(2)).await;
//             let last_msg_time = *last_message_time_clone.lock().await;
//             if last_msg_time.elapsed() > Duration::from_secs(2) {
//                 debug!("Endpoint '{}': No messages received in the last 2 seconds.", endpoint_clone);
//             } else {
//                 debug!("Endpoint '{}': Connection is alive.", endpoint_clone);
//             }
//         }
//     });
//
//     while let Some(message_result) = identified_read.next().await {
//         match message_result {
//             Ok(Message::Text(text)) => {
//                 debug!("Endpoint '{}': Received text message: {}", endpoint, text);
//                 *last_message_time.lock().await = Instant::now();
//                 let work = ConnectionMessage {
//                     endpoint: Some(endpoint.clone()),
//                     connection_timestamp,
//                     message: text,
//                 };
//                 if broadcaster.send(work).is_err() {
//                     warn!("Error broadcasting text message");
//                 }
//             },
//             Ok(Message::Binary(bin)) => {
//                 debug!("Endpoint '{}': Received binary message: {:?}", endpoint, bin);
//                 *last_message_time.lock().await = Instant::now();
//             },
//             Ok(Message::Ping(ping)) => {
//                 debug!("Endpoint '{}': Received ping: {:?}", endpoint, ping);
//                 *last_message_time.lock().await = Instant::now();
//             },
//             Ok(Message::Pong(pong)) => {
//                 debug!("Endpoint '{}': Received pong: {:?}", endpoint, pong);
//                 *last_message_time.lock().await = Instant::now();
//             },
//             Ok(Message::Close(close_frame)) => {
//                 debug!("Endpoint '{}': Received close frame: {:?}", endpoint, close_frame);
//                 break; // Typically you want to break the loop if close frame is received
//             },
//             Ok(other) => {
//                 debug!("Endpoint '{}': Received other message: {:?}", endpoint, other);
//                 *last_message_time.lock().await = Instant::now();
//             },
//             Err(e) => {
//                 error!("Endpoint '{}': Error reading message: {:?}", endpoint, e);
//                 break; // Break if there's an error reading message
//             }
//         }
//     }
//
//     info!("Broadcast task for endpoint '{}' has ended.", endpoint);
// }
