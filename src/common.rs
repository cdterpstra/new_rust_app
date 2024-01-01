// common.rs

use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug)]
pub struct WebSocketConnection {
    pub sender: mpsc::Sender<Message>,
    pub endpoint_name: String,
}
