use log::{info, error, debug};
use serde_json::{json, Value};
use tokio::{spawn, time};
use tokio::sync::{mpsc, broadcast};
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;
use crate::common::{StartPingMessage, BroadcastMessage, PongStatus};



#[derive(Debug, Clone)]
struct PingTask {
    endpoint_name: String,
    ws_sender: mpsc::Sender<Message>,
    broadcaster: broadcast::Sender<BroadcastMessage>,
    ping_status_sender: mpsc::Sender<PongStatus>, // Include success_sender in PingTask
}

impl PingTask {
    async fn start_pinging(self) {
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

            let req_id_for_verification = req_id.clone();
            let endpoint_name_for_verification = self.endpoint_name.clone();
            let broadcast_receiver = self.broadcaster.subscribe(); // Get a receiver to pass to verify_pong

            if let Err(e) = self.ws_sender.send(Message::Text(ping_message_json_string)).await {
                error!("Failed to send ping: {:?}", e);
                break;
            }

            let success_sender_clone = self.ping_status_sender.clone(); // Clone success_sender for use in the spawned task
            spawn(async move {
                verify_pong(broadcast_receiver, req_id_for_verification, endpoint_name_for_verification, success_sender_clone).await;
            });

            info!("Ping sent to {} with req_id {}", self.endpoint_name, req_id);
        }
    }
}

pub async fn ping_manager(mut ping_request_receiver: mpsc::Receiver<StartPingMessage>,
                          broadcaster: broadcast::Sender<BroadcastMessage>,
                          ping_status_sender: mpsc::Sender<PongStatus>) {  // Add success_sender here
    while let Some(start_message) = ping_request_receiver.recv().await {
        let ping_task = PingTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
            broadcaster: broadcaster.clone(),
            ping_status_sender: ping_status_sender.clone(),  // Use the success_sender from the function's parameters
        };
        spawn(ping_task.start_pinging());
    }
}

async fn verify_pong(mut broadcast_receiver: broadcast::Receiver<BroadcastMessage>,
                     expected_req_id: String,
                     endpoint_name: String,
                     ping_status_sender: mpsc::Sender<PongStatus>) {
    while let Ok(broadcast_msg) = broadcast_receiver.recv().await {
        if let Message::Text(text) = &broadcast_msg.message {
            match serde_json::from_str::<Value>(text) {
                Ok(json) => {
                    if json["op"] == "ping" && json["req_id"] == expected_req_id && json["success"] == true {
                        info!("Valid pong received for endpoint '{}' with req_id: {}", endpoint_name, expected_req_id);

                        let success_message = PongStatus {
                            endpoint_name: endpoint_name.clone(),
                            timestamp: broadcast_msg.timestamp, // Correctly access timestamp from the broadcast_msg
                            message: "Ping/Pong: Connection healthy".to_string(),
                        };

                        if let Err(e) = ping_status_sender.send(success_message).await {
                            error!("Ping/Pong: Connection unhealthy {:?}", e);
                        }
                        break;
                    }
                },
                Err(e) => {
                    error!("Failed to parse incoming message as JSON: {:?}", e);
                }
            }
        }
    }
}