use tokio_tungstenite::tungstenite::protocol::Message;
use tokio::sync::mpsc;
use log::{info, error, debug};
use tokio::{spawn, time};
use serde_json::json;
use uuid::Uuid;
use crate::websocket_manager::StartPingMessage;

#[derive(Debug, Clone)]
struct PingTask {
    endpoint_name: String,
    ws_sender: mpsc::Sender<Message>,
}

pub async fn ping_manager(mut receiver: mpsc::Receiver<StartPingMessage>) {
    debug!("Ping manager started."); // Log when the ping manager starts
    while let Some(start_message) = receiver.recv().await {
        debug!("Received StartPingMessage: {:?}", start_message); // Log when a message is received
        let ping_task = PingTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
        };
        spawn(ping_task.start_pinging());
    }
}

impl PingTask {
    async fn start_pinging(self) {
        debug!("Starting pinging task for endpoint {}", self.endpoint_name); // Log when a pinging task starts
        let mut interval = time::interval(time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            let req_id = Uuid::new_v4().to_string();
            let ping_message_json_string = json!({
                "op": "ping",
                "args": null,
                "req_id": req_id,
            })
                .to_string();

            debug!("Sending ping message: {}", ping_message_json_string);

            if let Err(e) = self.ws_sender.send(Message::Text(ping_message_json_string)).await {
                error!("Failed to send ping: {:?}", e);
                debug!("Stopping pinging task for endpoint {} due to error.", self.endpoint_name); // Log when stopping due to error
                break;
            }

            info!("Ping sent to {} with req_id {}", self.endpoint_name, req_id);
        }
    }
}