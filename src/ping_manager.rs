use log::{info, error, debug};
use serde_json::{json, Value};
use tokio::{spawn, time};
use tokio::sync::{mpsc, broadcast};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;
use crate::common::{StartPingMessage, BroadcastMessage};



#[derive(Debug, Clone)]
struct PingTask {
    endpoint_name: String,
    ws_sender: mpsc::Sender<Message>,
    broadcaster: broadcast::Sender<BroadcastMessage>,
}

impl PingTask {
    async fn start_pinging(self) {
        let mut interval = time::interval(time::Duration::from_secs(15));
        loop {
            interval.tick().await; // Await until the interval elapses
            let req_id = Uuid::new_v4().to_string();
            let ping_message_json_string = json!({
                "op": "ping",
                "args": null,
                "req_id": req_id,
            })
                .to_string();

            debug!("Sending ping message: {}", ping_message_json_string);

            let req_id_for_verification = req_id.clone();
            if let Err(e) = self.ws_sender.send(Message::Text(ping_message_json_string)).await {
                error!("Failed to send ping: {:?}", e);
                break;
            }

            let endpoint_name_for_verification = self.endpoint_name.clone(); // Clone endpoint name
            let receiver = self.broadcaster.subscribe();
            spawn(async move {
                verify_pong(receiver, req_id_for_verification, endpoint_name_for_verification).await; // Pass the cloned endpoint name
            });

            info!("Ping sent to {} with req_id {}", self.endpoint_name, req_id);
        }
    }
}

pub async fn ping_manager(mut receiver: mpsc::Receiver<StartPingMessage>, broadcaster: broadcast::Sender<BroadcastMessage>) {
    while let Some(start_message) = receiver.recv().await {
        let ping_task = PingTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
            broadcaster: broadcaster.clone(),
        };
        spawn(ping_task.start_pinging());
    }
}

async fn verify_pong(mut receiver: broadcast::Receiver<BroadcastMessage>, expected_req_id: String, endpoint_name: String) {
    while let Ok(broadcast_msg) = receiver.recv().await {
        if let Message::Text(text) = &broadcast_msg.message {
            match serde_json::from_str::<Value>(&text) {
                Ok(json) => {
                    if json["op"] == "ping" && json["req_id"] == expected_req_id && json["success"] == true {
                        info!("Valid pong received for endpoint '{}' with req_id: {}", endpoint_name, expected_req_id);
                        break; // Stop listening after a valid pong is received
                    }
                },
                Err(e) => {
                    error!("Failed to parse incoming message as JSON: {:?}", e);
                }
            }
        }
    }
}
