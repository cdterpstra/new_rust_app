// websocket_manager.rs

use tokio::sync::Notify;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::broadcast::Sender as Broadcaster;
use tokio::spawn;
use crate::broadcast_task::broadcast_task;
use crate::write_task::write_task;
use crate::types::MessageWithWSID; // Adjust this import based on your project structure
use uuid::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use futures_util::StreamExt;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::WebSocketStream;
use log::{debug, info, warn};
use tokio::time::{self, Duration};

struct ConnectionInfo {
    sender: Sender<String>,
    is_connected: Arc<AtomicBool>,
}

pub async fn websocket_manager(base_url: &str, endpoints: Vec<&str>, mut global_sender: Receiver<MessageWithWSID>, broadcaster: Broadcaster<MessageWithWSID>) {
    let connections = Arc::new(Mutex::new(HashMap::<String, ConnectionInfo>::new()));
    info!("WebSocket Manager started.");

    let uris: Vec<String> = endpoints.iter().map(|endpoint| format!("{}{}", base_url, endpoint)).collect();
    debug!("URIs for connection: {:?}", uris);

    // Iterate over the URIs and establish connections
    for uri in uris.iter() {
        debug!("Attempting to connect to {}", uri);
        let connection_id = Uuid::new_v4().to_string(); // Generate a unique identifier for the connection

        let (sender, receiver) = channel::<String>(32);
        let ws_stream = match ws_connection_manager(uri).await {
            Ok(stream) => {
                info!("Connected to {}", uri);
                stream
            },
            Err(e) => {
                warn!("Failed to connect to {}: {}", uri, e);
                continue;
            }
        };

        let (ws_write, ws_read) = ws_stream.split();

        let connection_info = ConnectionInfo {
            sender: sender.clone(),
            is_connected: Arc::new(AtomicBool::new(true)),
        };

        let mut conns = connections.lock().await;
        conns.insert(connection_id.clone(), connection_info);
        info!("Connection established and managed: {}", connection_id);

        let connection_id_clone = connection_id.clone();

        let identified_read = ws_read.map(move |message| { // use 'move' to move the cloned connection_id into the closure
            MessageWithWSID {
                connection_id: connection_id_clone.clone(), // Use the cloned connection_id
                content: message.unwrap().to_text().unwrap().to_string(),
            }
        });

        let broadcaster_clone = broadcaster.clone();
        spawn(broadcast_task(identified_read, broadcaster.clone()));

        // Access the is_connected from the map when needed
        let is_connected_clone = conns.get(&connection_id).unwrap().is_connected.clone();
        spawn(write_task(ws_write, receiver, is_connected_clone));
    }

    // Clone the connections Arc to use in the global sender handler
    let connections_clone = connections.clone();
    spawn(async move {
        info!("Global sender handler started.");
        let conns = connections_clone.lock().await;
        while let Some(message) = global_sender.recv().await {
            debug!("Handling message for connection_id: {}", message.connection_id);
            if let Some(connection_info) = conns.get(&message.connection_id) {
                if connection_info.is_connected.load(Ordering::SeqCst) {
                    match connection_info.sender.send(message.content.clone()).await {
                        Ok(_) => debug!("Message sent to {}", message.connection_id),
                        Err(e) => warn!("Failed to send message to {}: {}", message.connection_id, e),
                    }
                } else {
                    warn!("Connection {} is not active.", message.connection_id);
                }
            } else {
                warn!("No connection info found for {}", message.connection_id);
            }
        }
    });

// Loop to monitor the `is_connected` flag of each connection
    loop {
        let mut all_connected = true;
        {
            let conns = connections.lock().await;
            for (_, info) in conns.iter() {
                if !info.is_connected.load(Ordering::SeqCst) {
                    all_connected = false;
                    break;
                }
            }
        }

        // If any connection is not active, break the loop
        if !all_connected {
            break;
        }

        // Sleep for a short duration before checking again
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // Perform any necessary cleanup here
    info!("At least one connection is no longer active. Cleaning up before websocket_manager exits.");
    // E.g., gracefully close connections, notify spawned tasks to terminate, etc.
}

async fn ws_connection_manager(uri: &str) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, String> {
    debug!("Connecting to WebSocket: {}", uri);
    // Attempt to connect and convert any errors into a String
    match connect_async(uri).await {
        Ok((stream, _)) => {
            info!("Connection established: {}", uri);
            Ok(stream) // Ignore the HTTP response part and return the stream
        },
        Err(e) => {
            warn!("Error connecting to {}: {}", uri, e);
            Err(e.to_string())  // Convert the error into a String
        }
    }
}
