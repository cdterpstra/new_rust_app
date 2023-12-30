use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::collections::HashMap;
use tokio::sync::{mpsc::{Receiver, Sender, channel}, broadcast::Sender as Broadcaster, RwLock, Mutex};
use tokio::{spawn, time::Duration};
use futures_util::StreamExt;
use log::{debug, info, warn, error};
use rand::Rng;
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;
use crate::broadcast_task::broadcast_task;
use crate::common::ConnectionMessage;
use crate::write_task::write_task;


pub struct ConnectionStates {
    pub(crate) public_spot: AtomicU32,
    pub(crate) public_linear: AtomicU32,
    pub(crate) public_inverse: AtomicU32,
    pub(crate) public_option: AtomicU32,
    pub(crate) private: AtomicU32,
}

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
    let work_sender = channel::<ConnectionMessage>(100).0; // Worker channel sender
    let work_receiver = channel::<ConnectionMessage>(100).1; // Worker channel receiver
    let work_receiver = Arc::new(Mutex::new(work_receiver)); // Wrap receiver in Arc<Mutex> for shared access

    let worker_count = 4; // Define the number of workers in the pool
    for _ in 0..worker_count {
        let work_receiver = Arc::clone(&work_receiver); // Clone the Arc for each worker
        let connection_map = Arc::clone(&connection_map); // Clone the Arc for each worker

        spawn(async move {
            while let Some(work_item) = work_receiver.lock().await.recv().await {
                // Process each work item
                if let Some(connection) = connection_map.read().await.get(&work_item.connection_id) {
                    if let Err(e) = connection.sender.send(work_item.message).await {
                        error!("Failed to send message to connection {}: {}", work_item.connection_id, e);
                    }
                } else {
                    warn!("No connection found for ID: {}", work_item.connection_id);
                }
            }
        });
    }

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        debug!("Attempting to connect to {}", uri);

        let random_number: u32 = rand::thread_rng().gen_range(0..2_147_483_647);
        connection_states.update_state(endpoint, random_number);
        let connection_id = random_number;

        let (sender, receiver) = channel::<String>(32);
        {
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
        let connection_id_clone = connection_id;

        let work_sender_clone = work_sender.clone(); // Clone the work sender for the broadcast task
        spawn(broadcast_task(
            ws_read,
            broadcaster_clone,
            work_sender_clone,
            connection_id_clone,
        ));

        spawn(write_task(ws_write, receiver));
    }

    spawn(async move {
        info!("Global sender handler started.");
        while let Some(connection_message) = global_sender.recv().await {
            if work_sender.send(connection_message).await.is_err() {
                error!("Failed to send work to the worker pool");
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
