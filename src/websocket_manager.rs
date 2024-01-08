use crate::listener;
use crate::ping_manager::start_pinging;
use crate::subscription_manager::start_subscribing;
use colored::Colorize;
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio::{select, spawn, time};
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream};

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
    args: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SubscribeMessage {
    success: Option<bool>,
    ret_msg: Option<String>,
    conn_id: Option<String>,
    req_id: Option<String>,
    op: String,
}

async fn handle_websocket_stream<S>(
    mut read: SplitStream<WebSocketStream<S>>,
    uri: &str,
    pong_tx: mpsc::Sender<MyMessage>,
    subscribe_response_tx: mpsc::Sender<MyMessage>,
    general_tx: mpsc::Sender<MyMessage>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    while let Some(message) = read.next().await {
        match message {
            Ok(msg) => {
                if let Message::Text(text) = msg {
                    // Parse message as a generic JSON Value
                    // debug!("Received message: {}", text.on_red());
                    match serde_json::from_str::<Value>(&text) {
                        Ok(value) => {
                            if let Some(op) = value["op"].as_str() {
                                // Check if the "op" field is "ping" or "pong"
                                if op == "ping" || op == "pong" {
                                    let parsed_msg: PongMessage = serde_json::from_value(value)
                                        .expect("Failed to parse message as PongMessage");
                                    debug!("Parsed message as PongMessage: {:?}", parsed_msg);

                                    let my_msg = MyMessage {
                                        timestamp: chrono::Utc::now().timestamp_millis() as u128,
                                        endpoint_name: uri.to_string(),
                                        message: Message::Text(text.clone()), // Repackaging text as Message
                                    };

                                    if let Err(e) = pong_tx.send(my_msg).await {
                                        error!("Error forwarding to pong handler: {:?}", e);
                                    }
                                } else if op == "subscribe" {
                                    // Handle "subscribe" messages
                                    let parsed_msg: SubscribeMessage =
                                        serde_json::from_value(value)
                                            .expect("Failed to parse message as SubscribeMessage");
                                    // debug!("Parsed message as SubscribeMessage: {:?}", parsed_msg);

                                    let my_msg = MyMessage {
                                        timestamp: chrono::Utc::now().timestamp_millis() as u128,
                                        endpoint_name: uri.to_string(),
                                        message: Message::Text(text.clone()), // Repackaging text as Message
                                    };

                                    debug!("Forwarding subscribe message: {:?}", my_msg);
                                    if let Err(e) = subscribe_response_tx.send(my_msg).await {
                                        error!("Error forwarding to subscribe handler: {:?}", e);
                                    }
                                } else {
                                    // Handle messages with other "op" fields
                                    forward_general_message(text, uri, &general_tx).await;
                                }
                            } else {
                                // Handle messages without an "op" field
                                forward_general_message(text, uri, &general_tx).await;
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse message: {:?}", e);
                            // Since the message couldn't be parsed, treat it as a general message
                            forward_general_message(text, uri, &general_tx).await;
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error receiving ws message: {:?}", e);
                break; // Exit the loop on error
            }
        }
    }
}

async fn forward_general_message(text: String, uri: &str, general_tx: &mpsc::Sender<MyMessage>) {
    let my_msg = MyMessage {
        timestamp: chrono::Utc::now().timestamp_millis() as u128,
        endpoint_name: uri.to_string(),
        message: Message::Text(text), // Repackaging text as Message
    };
    // debug!("Forwarding general message: {:?}", my_msg);

    if let Err(e) = general_tx.send(my_msg).await {
        error!("Error forwarding to general handler: {:?}", e);
    }
}

pub async fn manage_connection(uri: String, general_tx: mpsc::Sender<MyMessage>) {
    let mut retry_delay = 1;
    let mut rng = rand::rngs::StdRng::from_entropy();
    debug!("Starting connection management for {}", uri);

    loop {
        debug!("Attempting to connect to {}", uri);
        match connect_async(&uri).await {
            Ok((ws_stream, _)) => {
                debug!("Successfully connected to {}", uri);
                retry_delay = 1; // Reset retry delay after successful connection

                let (mut write, read) = ws_stream.split();
                debug!("WebSocket stream split into write and read");

                let (ping_tx, mut ping_rx) = mpsc::channel::<MyMessage>(32);
                let (pong_tx, pong_rx) = mpsc::channel::<MyMessage>(32);
                let (subscribe_request_tx, mut subscribe_request_rx) =
                    mpsc::channel::<MyMessage>(32);
                let (subscribe_response_tx, subscribe_response_rx) = mpsc::channel::<MyMessage>(32);
                debug!("Channels created");

                let uri_clone = uri.clone();
                let pong_tx_clone = pong_tx.clone();
                let subscribe_response_tx_clone = subscribe_response_tx.clone();
                let general_tx_clone = general_tx.clone();

                debug!("Spawning task to handle WebSocket stream");
                let handle_task = spawn(async move {
                    handle_websocket_stream(
                        read,
                        &uri_clone,
                        pong_tx_clone,
                        subscribe_response_tx_clone,
                        general_tx_clone,
                    )
                    .await;
                });

                debug!("Spawning task to write messages");
                let _write_task = spawn(async move {
                    loop {
                        select! {
                            Some(my_msg) = ping_rx.recv() => {
                                 if let Err(e) = write.send(my_msg.message).await {
                                 error!("Failed to send ping to WebSocket: {:?}", e);
                                 break;
                                 } else {
                                debug!("Successfully sent ping message to WebSocket");
                                    }
                                },
                            Some(subscribe_msg) = subscribe_request_rx.recv() => {
                                let message_clone = subscribe_msg.message.clone(); // Clone the message
                                if let Err(e) = write.send(message_clone).await {
                                error!("Failed to send subscription message to WebSocket: {:?}", e);
                                break;
                                } else {
                                debug!("Successfully sent subscription message to WebSocket: {:?}", subscribe_msg.message);
                            }
                        },

                        else => break,
                                                }
                    }
                });

                let uri_for_ping_task = uri.clone();
                debug!("Spawning pinging task for {}", uri_for_ping_task);
                let _ping_task = spawn(async move {
                    start_pinging(ping_tx, pong_rx, uri_for_ping_task).await;
                });

                let uri_for_subscribe_task = uri.clone();
                debug!("Spawning subscription task for {}", uri_for_subscribe_task);
                let _subscription_task = spawn(async move {
                    start_subscribing(
                        subscribe_request_tx,
                        subscribe_response_rx,
                        uri_for_subscribe_task,
                    )
                    .await;
                });

                debug!(
                    "Awaiting the completion of WebSocket handling task for {}",
                    uri
                );
                let _ = handle_task.await;
                debug!("WebSocket handling task completed for {}", uri);
            }
            Err(e) => {
                error!("Failed to connect to {}: {:?}", uri, e);
            }
        }

        let jitter = rng.gen_range(0..6);
        let sleep_time = std::cmp::min(retry_delay, 1024) + jitter;
        debug!(
            "Waiting {} seconds before retrying connection to {}",
            sleep_time, uri
        );
        time::sleep(time::Duration::from_secs(sleep_time)).await;
        retry_delay *= 2;
    }
}

pub async fn websocket_manager(base_url: &str, endpoints: &Vec<String>) {
    debug!("Initializing WebSocket manager");

    // Create a channel for general messages
    let (general_tx, general_rx) = mpsc::channel::<MyMessage>(32);

    // Spawn the listener task with its own receiver clone
    spawn(async move {
        listener::listen_for_messages(general_rx).await;
    });

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        debug!("Creating WebSocket connection for {}", uri);

        // Spawn a connection management task
        spawn(manage_connection(uri, general_tx.clone()));
    }
}
