use futures_util::{StreamExt, SinkExt};
use futures_util::stream::SplitStream;
use log::{debug, error};
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::{spawn, time};
use tokio_tungstenite::{connect_async, WebSocketStream, tungstenite::Message};
use tokio::io::{AsyncRead, AsyncWrite};
use crate::ping_manager::start_pinging;


// Define a structure for messages
#[derive(Debug)]
pub struct MyMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub message: Message,
}

// Updated handle_websocket_stream function
#[derive(Serialize, Deserialize, Debug)]
struct PongMessage {
    success: Option<bool>,
    ret_msg: Option<String>,
    conn_id: Option<String>,
    req_id: Option<String>,
    op: String,
    args: Option<Vec<String>>
}

async fn handle_websocket_stream<S>(
    mut read: SplitStream<WebSocketStream<S>>,
    uri: &str,
    pong_tx: mpsc::Sender<MyMessage>,
    general_tx: mpsc::Sender<MyMessage>,
)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    while let Some(message) = read.next().await {
        match message {
            Ok(msg) => {
                // Convert WebSocket Message to text if possible
                if let Message::Text(text) = msg {
                    match serde_json::from_str::<PongMessage>(&text) {
                        Ok(parsed_msg) => {
                            debug!("Parsed message: {:?}", parsed_msg);
                            // Check if the "op" field is "ping" or "pong" and forward it through the pong_tx channel
                            if parsed_msg.op == "ping" || parsed_msg.op == "pong" {
                                let my_msg = MyMessage {
                                    timestamp: chrono::Utc::now().timestamp_millis() as u128,
                                    endpoint_name: uri.to_string(),
                                    message: Message::Text(text.clone()), // Repackaging text as Message
                                };

                                if let Err(e) = pong_tx.send(my_msg).await {
                                    error!("Error forwarding to pong handler: {:?}", e);
                                }
                            } else {
                                // Forward other messages to general handler
                                forward_general_message(text, uri, &general_tx).await;
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse message: {:?}", e);
                            forward_general_message(text, uri, &general_tx).await;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error receiving ws message: {:?}", e);
                break;
            }
        }
    }
}

async fn forward_general_message(
    text: String,
    uri: &str,
    general_tx: &mpsc::Sender<MyMessage>,
) {
    let my_msg = MyMessage {
        timestamp: chrono::Utc::now().timestamp_millis() as u128,
        endpoint_name: uri.to_string(),
        message: Message::Text(text), // Repackaging text as Message
    };
    if let Err(e) = general_tx.send(my_msg).await {
        error!("Error forwarding to general tasks manager: {:?}", e);
    }
}

pub async fn manage_connection(
    uri: String,
    general_tx: mpsc::Sender<MyMessage>,
) {
    let mut retry_delay = 1;
    let mut rng = StdRng::from_entropy();
    debug!("Starting connection management for {}", uri);

    loop {
        debug!("Attempting to connect to {}", uri);
        match connect_async(&uri).await {
            Ok((ws_stream, _)) => {
                debug!("Successfully connected to {}", uri);
                retry_delay = 1; // Reset retry delay after successful connection

                let (mut write, read) = ws_stream.split();
                debug!("WebSocket stream split into write and read");

                // Creating channels for ping/pong message communication
                let (ping_tx, mut ping_rx) = mpsc::channel::<MyMessage>(32);
                let (pong_tx, mut pong_rx) = mpsc::channel::<MyMessage>(32);
                debug!("Ping and Pong channels created");

                let uri_clone = uri.clone();
                let pong_tx_clone = pong_tx.clone();
                let general_tx_clone = general_tx.clone();

                debug!("Spawning task to handle WebSocket stream");
                let handle_task = spawn(async move {
                    handle_websocket_stream(read, &uri_clone, pong_tx_clone, general_tx_clone).await;
                });

                debug!("Spawning task to write ping messages");
                let write_task = spawn(async move {
                    while let Some(my_msg) = ping_rx.recv().await {
                        if let Err(e) = write.send(my_msg.message).await {
                            error!("Failed to send ping to WebSocket: {:?}", e);
                            // Log and/or handle error as needed
                            break;
                        }
                    }
                });

                let uri_for_async = uri.clone();
                debug!("Spawning pinging task for {}", uri_for_async);
                let ping_task = spawn(async move {
                    start_pinging(ping_tx, pong_rx, uri_for_async).await;
                });

                // Await the completion of the connection handling task
                debug!("Awaiting the completion of WebSocket handling task for {}", uri);
                let _ = handle_task.await;
                debug!("WebSocket handling task completed for {}", uri);

                // You might want to handle or await the completion of write_task and ping_task here

                debug!("Connection lost to {}. Attempting to reconnect...", uri);
            }
            Err(e) => {
                error!("Failed to connect to {}: {:?}", uri, e);
                debug!("Retrying connection to {} after {} seconds", uri, retry_delay);
            }
        }

        // Exponential backoff with jitter for reconnection attempts
        let jitter = rng.gen_range(0..6);
        let sleep_time = std::cmp::min(retry_delay, 1024) + jitter;
        debug!("Waiting {} seconds before retrying connection to {}", sleep_time, uri);
        time::sleep(time::Duration::from_secs(sleep_time)).await;
        retry_delay *= 2; // Increase the delay for the next retry
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
) {
    debug!("Initializing WebSocket manager");

    // Create a channel for general messages
    let (general_tx, general_rx) = mpsc::channel::<MyMessage>(32);

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);

        // Create individual channels for pong messages for each connection
        let (pong_tx, pong_rx) = mpsc::channel(32);

        let general_tx_clone = general_tx.clone();

        spawn(manage_connection(
            uri,
            pong_tx,
        ));

        // Handle pong messages or other tasks specific to the connection here if needed
        // ...
    }

    // Here, you can listen to general_rx for incoming general messages
    // and handle them as needed in your application
    // while let Some(msg) = general_rx.recv().await {
    //     // Handle general message
    // }
}

// Your main function or other parts of your application would go here
// async fn main() {
//     // Initialize logging, WebSocket manager, or other components here.
// }
