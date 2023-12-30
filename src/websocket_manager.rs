use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::collections::HashMap;
use tokio::sync::{mpsc::{Receiver, Sender, channel}, broadcast::Sender as Broadcaster, RwLock};
use tokio::{spawn, time::Duration};
use futures_util::StreamExt;
use log::{debug, info, warn, error};
use rand::Rng;
use crate::broadcast_task::broadcast_task;
use crate::write_task::write_task;
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;


pub struct ConnectionStates {
    pub(crate) public_spot: AtomicU32,
    pub(crate) public_linear: AtomicU32,
    pub(crate) public_inverse: AtomicU32,
    pub(crate) public_option: AtomicU32,
    pub(crate) private: AtomicU32,
}

// Struct to hold both message and its associated connection ID
pub struct ConnectionMessage {
    connection_id: u32,
    message: String,
}

// New struct representing a connection with its own lock
pub struct Connection {
    sender: Sender<String>,
}

impl Connection {
    pub fn new(sender: Sender<String>) -> Self {
        Connection {
            sender,
        }
    }
}



impl ConnectionStates {
    pub fn new() -> Self {
        debug!("Initializing new connection states");
        ConnectionStates {
            public_spot: AtomicU32::new(0),
            public_linear: AtomicU32::new(0),
            public_inverse: AtomicU32::new(0),
            public_option: AtomicU32::new(0),
            private: AtomicU32::new(0),
        }
    }

    pub fn update_state(&self, endpoint: &str, value: u32) {
        debug!("Updating state for endpoint: {}, value: {}", endpoint, value);
        match endpoint {
            "public/spot" => self.public_spot.store(value, Ordering::SeqCst),
            "public/linear" => self.public_linear.store(value, Ordering::SeqCst),
            "public/inverse" => self.public_inverse.store(value, Ordering::SeqCst),
            "public/option" => self.public_option.store(value, Ordering::SeqCst),
            "private" => self.private.store(value, Ordering::SeqCst),
            _ => warn!("Attempted to update state for unknown endpoint: {}", endpoint),
        }
    }

    pub fn reset_state(&self, endpoint: &str) {
        debug!("Resetting state for endpoint: {}", endpoint);
        self.update_state(endpoint, 0);
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    mut global_sender: Receiver<ConnectionMessage>,
    broadcaster: Broadcaster<String>,
) {
    let connection_states = Arc::new(ConnectionStates::new());
    let connection_map = Arc::new(RwLock::new(HashMap::<u32, Connection>::new()));
    info!("WebSocket Manager started.");

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        debug!("Attempting to connect to {}", uri);

        let random_number: u32 = rand::thread_rng().gen_range(0..2_147_483_647);
        connection_states.update_state(endpoint, random_number);
        let connection_id = random_number; // Using random number as connection ID

        let (sender, receiver) = channel::<String>(32);

        {
            // Acquire write lock for modifying the map
            let mut conn_map = connection_map.write().await;
            debug!("Inserting new connection into map: {}", connection_id);
            conn_map.insert(connection_id, Connection::new(sender));
        }


        let ws_stream = match ws_connection_manager(&uri).await {
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

        let broadcaster_clone = broadcaster.clone();
        let connection_id_clone = connection_id.to_string();

        let identified_read = ws_read.map(move |message_result| {
            match message_result {
                Ok(message) => {
                    debug!("Message received on WebSocket {}: {}", connection_id_clone, message);
                    format!("{}: {}", connection_id_clone, message.into_text().unwrap_or_else(|_| "Error in message conversion".to_string()))
                },
                Err(e) => {
                    warn!("WebSocket read error on {}: {:?}", connection_id_clone, e);
                    format!("{}: Error", connection_id_clone)
                }
            }
        });

        spawn(broadcast_task(identified_read, broadcaster_clone));
        spawn(write_task(ws_write, receiver));
    }

    spawn(async move {
        info!("Global sender handler started.");
        while let Some(ConnectionMessage { connection_id, message }) = global_sender.recv().await {
            let conn_map = connection_map.read().await;
            if let Some(connection) = conn_map.get(&connection_id) {
                match connection.sender.send(message).await {
                    Ok(_) => debug!("Message sent to connection {}", connection_id),
                    Err(e) => error!("Failed to send message to connection {}: {}", connection_id, e),
                }
            } else {
                warn!("No connection found for ID: {}", connection_id);
            }
        }
    });

    loop {
        let all_connected = {
            connection_states.public_spot.load(Ordering::SeqCst) != 0 &&
                connection_states.public_linear.load(Ordering::SeqCst) != 0 &&
                connection_states.public_inverse.load(Ordering::SeqCst) != 0 &&
                connection_states.public_option.load(Ordering::SeqCst) != 0 &&
                connection_states.private.load(Ordering::SeqCst) != 0
        };

        if !all_connected {
            info!("Not all connections are active. Exiting monitoring loop.");
            break;
        }

        debug!("All connections are active. Continuing to monitor.");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    info!("At least one connection is no longer active. Cleaning up before websocket_manager exits.");
    // Perform any necessary cleanup here
}

async fn ws_connection_manager(uri: &str) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
    debug!("Connecting to WebSocket: {}", uri);
    match connect_async(uri).await {
        Ok((stream, _)) => {
            info!("Connection established: {}", uri);
            Ok(stream)
        },
        Err(e) => {
            warn!("Error connecting to {}: {}", uri, e);
            Err(e.to_string())
        }
    }
}
