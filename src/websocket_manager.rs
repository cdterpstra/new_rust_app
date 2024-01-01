use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, broadcast};
use log::{info, error};
use futures_util::{StreamExt, SinkExt};
use std::collections::HashMap;
use tokio::spawn;
use crate::common::WebSocketConnection;


#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub message: Message,
}

async fn manage_connection(
    uri: String,
    global_broadcaster: broadcast::Sender<BroadcastMessage>,
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>
) {
    match connect_async(&uri).await {
        Ok((ws_stream, _)) => {
            info!("Connected to {}", uri);
            let (tx, mut rx) = mpsc::channel(32);

            let timestamp = chrono::Utc::now().timestamp_millis() as u128;
            connection_map.write().await.insert(timestamp, WebSocketConnection {
                sender: tx.clone(),
                endpoint_name: uri.clone(),
            });

            let (mut write, mut read) = ws_stream.split();

            spawn(async move {
                while let Some(message) = rx.recv().await {
                    write.send(message).await.expect("Failed to write message");
                }
            });

            while let Some(message) = read.next().await {
                match message {
                    Ok(msg) => {
                        let broadcast_msg = BroadcastMessage {
                            timestamp,
                            endpoint_name: uri.clone(),
                            message: msg,
                        };
                        global_broadcaster.send(broadcast_msg).expect("Failed to broadcast message");
                    },
                    Err(e) => {
                        error!("Error receiving ws message: {:?}", e);
                        break;
                    }
                }
            }
        },
        Err(e) => {
            error!("Failed to connect to {}: {:?}", uri, e);
        }
    }
}

pub async fn websocket_manager(
    base_url: &str,
    endpoints: Vec<&str>,
    connection_map: Arc<RwLock<HashMap<u128, WebSocketConnection>>>,
    global_broadcaster: broadcast::Sender<BroadcastMessage>
) {
    for endpoint in endpoints.iter() {
        let uri = format!("{}{}", base_url, endpoint);
        spawn(manage_connection(uri, global_broadcaster.clone(), connection_map.clone()));
    }
}

