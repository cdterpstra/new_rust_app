// common.rs

use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;

#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub message: Message,
}
#[derive(Debug)]
pub struct BroadcastMessageFout {
    pub timestamp: String,
    pub endpoint_name: String,
    pub message: Message,
}

#[derive(Debug, Clone)]
pub struct StartTaskMessage {
    pub timestamp: u128,
    pub endpoint_name: String,
    pub ws_sender: mpsc::Sender<Message>,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub endpoint_name: String,
    pub timestamp: u128,
    pub message: String,
    pub sending_party: String,
}

#[derive(Debug)]
pub struct ManageTask {
    pub endpoint_name: String,
    pub ws_sender: mpsc::Sender<Message>,
    pub broadcaster: Receiver<BroadcastMessage>,
    pub status_sender: Sender<Status>,
}

#[derive(Debug)]
pub struct ManageTaskFout {
    pub endpoint_name: String,
    pub ws_sender: mpsc::Sender<Message>,
    pub broadcaster: Receiver<BroadcastMessageFout>,
    pub status_sender: Sender<Status>,
}