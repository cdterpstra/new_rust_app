use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use futures_util::StreamExt;
use tokio::sync::{mpsc::{Receiver, Sender, channel}, broadcast::Sender as Broadcaster, RwLock, Mutex};
use tokio::{spawn};
use log::{debug, info, warn, error};
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;
use crate::broadcast_task::broadcast_task;
use crate::common::ConnectionMessage;
use crate::write_task::write_task;

#[derive(Debug)]
pub struct Connection {
    pub(crate) sender: Sender<String>,
    pub(crate) endpoint_name: String, // Field to store endpoint name
}

impl Connection {
    pub fn new(sender: Sender<String>, endpoint_name: String) -> Self {
        Connection { sender, endpoint_name }
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    mut global_sender: Receiver<ConnectionMessage>,
    broadcaster: Broadcaster<ConnectionMessage>,
    signal_sender: Sender<String>,
    connection_map: Arc<RwLock<HashMap<u128, Connection>>>,
) {
    debug!("websocket_manager connection_map Arc pointer address: {:?}", Arc::as_ptr(&connection_map));
    debug!("websocket_manager connection_map Arc strong count: {}", Arc::strong_count(&connection_map));

    let work_sender = channel::<ConnectionMessage>(100).0;
    let work_receiver = channel::<ConnectionMessage>(100).1;
    let work_receiver = Arc::new(Mutex::new(work_receiver));

    let worker_count = 4;
    for _ in 0..worker_count {
        let work_receiver_clone = Arc::clone(&work_receiver);
        let connection_map_clone = Arc::clone(&connection_map);

        spawn(async move {
            while let Some(work_item) = work_receiver_clone.lock().await.recv().await {
                if let Some(connection) = connection_map_clone.read().await.get(&work_item.connection_timestamp) {
                    if let Err(e) = connection.sender.send(work_item.message).await {
                        error!("Failed to send message to connection {}: {}", work_item.connection_timestamp, e);
                    }
                } else {
                    warn!("No connection found for ID: {}", work_item.connection_timestamp);
                }
            }
        });
    }

    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        debug!("Attempting to connect to {}", uri);

        let connection_timestamp: u128 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let (sender, receiver) = channel::<String>(32);

        let mut conn_map = connection_map.write().await;
        let key = connection_timestamp;

        conn_map.insert(key, Connection::new(sender, endpoint.to_string()));
        debug!("New connection added with timestamp: {}. Current state of connection_map: {:?}", key, conn_map);
        debug!("After insertion, connection_map Arc pointer address: {:?}", Arc::as_ptr(&connection_map));
        debug!("After insertion, connection_map Arc strong count: {}", Arc::strong_count(&connection_map));

        let ws_stream = match ws_connection_manager(&uri, signal_sender.clone(), connection_timestamp).await {
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
        let connection_timestamp_clone = connection_timestamp;

        let work_sender_clone = work_sender.clone();
        spawn(broadcast_task(
            ws_read,
            broadcaster_clone,
            work_sender_clone,
            endpoint.to_string(),
            connection_timestamp_clone,
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
}

async fn ws_connection_manager(
    uri: &str,
    signal_sender: Sender<String>,
    connection_timestamp: u128,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
    debug!("Connecting to WebSocket: {}", uri);
    match connect_async(uri).await {
        Ok((stream, _)) => {
            let message = format!("Connection established for:{}", connection_timestamp);
            if let Err(e) = signal_sender.send(message.clone()).await {
                error!("Failed to send connection established message for {}: {:?}", connection_timestamp, e);
            }
            Ok(stream)
        },
        Err(e) => {
            warn!("Error connecting to {}: {}", uri, e);
            Err(e.to_string())
        }
    }
}
