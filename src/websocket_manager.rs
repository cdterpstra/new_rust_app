use std::sync::Arc;
use colored::Colorize;
use crate::{listener, write_to_database};
use crate::ping_manager::start_pinging;
use crate::subscription_manager::start_subscribing;
use crate::websocket_handler::handle_websocket_stream;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error};
use rand::{Rng, SeedableRng, thread_rng};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::{select, spawn, time};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use crate::db_connection_manager::establish_connection;


// Define a structure for messages
#[derive(Debug)]
pub struct MyMessage {
    pub receivedat: i64,
    pub endpoint_name: String,
    pub message: Message,
}

// Updated handle_websocket_stream function
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct PongMessage {
    success: Option<bool>,
    ret_msg: Option<String>,
    conn_id: Option<String>,
    req_id: Option<String>,
    op: String,
    args: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct SubscribeMessage {
    success: Option<bool>,
    ret_msg: Option<String>,
    conn_id: Option<String>,
    req_id: Option<String>,
    op: String,
}



pub(crate) async fn forward_general_message(text: String, uri: &str, general_tx: &mpsc::Sender<MyMessage>) {
    let timestamp: i64 = match chrono::Utc::now().timestamp_nanos_opt() {
        Some(nanos) => nanos,
        None => {
            // Handle the error appropriately, such as logging or panicking
            panic!("Unable to obtain timestamp in nanoseconds.");
        }
    };
    let my_msg = MyMessage {
        receivedat: timestamp as i64,
        endpoint_name: uri.to_string(),
        message: Message::Text(text), // Repackaging text as Message
    };
    debug!("Forwarding general message: {:?}", my_msg);

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
                                let message_clone = my_msg.message.clone();
                                 if let Err(e) = write.send(message_clone).await {
                                 error!("Failed to send ping to WebSocket: {:?}", e);
                                 break;
                                 } else {
                                debug!("Successfully sent ping message to WebSocket: {:?}", my_msg.message);
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
                    let _ = start_subscribing(
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

pub async fn websocket_manager(base_url: &str, endpoints: &[String]) {
    debug!("Initializing WebSocket manager");

    // Create a channel for general messages
    let (general_tx, general_rx) = mpsc::channel::<MyMessage>(32);

    // Shared state to manage connections
    let connections = Arc::new(Mutex::new(Vec::new()));

    // // Spawn the listener task with its own receiver clone
    // spawn(async move {
    //     listener::listen_for_messages(general_rx).await;
    // });


    // Spawn the database writer task with its own receiver clone
    let conn = establish_connection();
    spawn(async move {
        write_to_database::insert_into_db(general_rx, conn).await;
    });

    // Main loop to manage connections
    loop {
        let mut conn_guard = connections.lock().await;
        conn_guard.clear(); // Disconnect existing connections

        for endpoint in endpoints.iter() {
            let uri = format!("{}{}", base_url, endpoint);
            debug!("Creating WebSocket connection for {}", uri);

            // Spawn a connection management task and keep the handle
            let handle = spawn(manage_connection(uri, general_tx.clone()));
            conn_guard.push(handle);
        }

        // Release the lock before sleeping
        drop(conn_guard);

        // Generate a random duration between 4 and 18 hours
        let mut rng = thread_rng();
        let random_duration = rng.gen_range(4 * 60 * 60..18 * 60 * 60);

        // Wait for the random duration before restarting the loop
        time::sleep(core::time::Duration::from_secs(random_duration)).await;
        println!("{}", "Restarting WebSocket connections".on_red());
    }
}