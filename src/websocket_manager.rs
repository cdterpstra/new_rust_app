use tokio::sync::{mpsc::{Receiver, Sender, channel}, broadcast::Sender as Broadcaster, Mutex};
use tokio::spawn;
use tokio::time::Duration;
use futures_util::StreamExt;
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::collections::HashMap;
use log::{debug, info, warn};
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

impl ConnectionStates {
    pub fn new() -> Self {
        ConnectionStates {
            public_spot: AtomicU32::new(0),
            public_linear: AtomicU32::new(0),
            public_inverse: AtomicU32::new(0),
            public_option: AtomicU32::new(0),
            private: AtomicU32::new(0),
        }
    }

    pub fn update_state(&self, endpoint: &str, value: u32) {
        match endpoint {
            "public/spot" => self.public_spot.store(value, Ordering::SeqCst),
            "public/linear" => self.public_linear.store(value, Ordering::SeqCst),
            "public/inverse" => self.public_inverse.store(value, Ordering::SeqCst),
            "public/option" => self.public_option.store(value, Ordering::SeqCst),
            "private" => self.private.store(value, Ordering::SeqCst),
            _ => {}
        }
    }

    pub fn reset_state(&self, endpoint: &str) {
        self.update_state(endpoint, 0);
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    mut global_sender: Receiver<String>,
    broadcaster: Broadcaster<String>,
) {
    let connection_states = Arc::new(ConnectionStates::new());
    info!("WebSocket Manager started.");

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        debug!("Attempting to connect to {}", uri);

        let random_number: u32 = rand::thread_rng().gen_range(0..2_147_483_647);
        connection_states.update_state(endpoint, random_number);
        let connection_id = random_number.to_string(); // Using random number as connection ID

        let (sender, receiver) = channel::<String>(32);

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
        let connection_id_clone = connection_id.clone();

        let identified_read = ws_read.map(move |message_result| {
            match message_result {
                Ok(message) => format!("{}: {}", connection_id_clone, message.into_text().unwrap_or_else(|_| "Error in message conversion".to_string())),
                Err(e) => {
                    warn!("WebSocket read error: {:?}", e);
                    format!("{}: Error", connection_id_clone)
                }
            }
        });

        spawn(broadcast_task(identified_read, broadcaster_clone));
        spawn(write_task(ws_write, receiver));
    }

    let connections_clone = Arc::new(Mutex::new(HashMap::<u32, Sender<String>>::new()));
    spawn(async move {
        info!("Global sender handler started.");
        while let Some(message) = global_sender.recv().await {
            // The logic to handle the message would go here
        }
    });

    // Monitoring loop for connection states
    loop {
        let all_connected = {
            // Check each atomic integer individually
            connection_states.public_spot.load(Ordering::SeqCst) != 0 &&
                connection_states.public_linear.load(Ordering::SeqCst) != 0 &&
                connection_states.public_inverse.load(Ordering::SeqCst) != 0 &&
                connection_states.public_option.load(Ordering::SeqCst) != 0 &&
                connection_states.private.load(Ordering::SeqCst) != 0
        };

        if !all_connected {
            break; // Exit loop if any connection is not active
        }

        tokio::time::sleep(Duration::from_secs(5)).await; // Sleep before checking again
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
