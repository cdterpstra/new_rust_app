// common.rs

use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub message: Message,
}

#[derive(Debug, Clone)]
pub struct StartPingMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub ws_sender: mpsc::Sender<Message>,
}

#[derive(Debug, Clone)]
pub struct PongStatus {
    pub endpoint_name: String,
    pub timestamp: u128,
    pub message: String,
}
