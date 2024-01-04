use log::{debug, error, info};
use serde_json::json;
use tokio::spawn;
use tokio::sync::mpsc;

use tokio::sync::broadcast::{Receiver, Sender};

use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
// subscription_manager.rs
use crate::common::{BroadcastMessage, Status, SubscriptionMessage};
// pub struct SubscriptionManager {
//     transmitter: broadcast::Sender<String>,
// }

#[derive(Debug)]
struct SubscriptionTask {
    endpoint_name: String,
    ws_sender: mpsc::Sender<Message>,
    broadcaster: Receiver<BroadcastMessage>,
    subscription_status_sender: Sender<Status>,
}

impl SubscriptionTask {
    async fn start_subscribing(self) {
        let req_id = Uuid::new_v4().to_string();
        let subscription_message_json_string = json!({
            "req_id": req_id,
            "op": "subscribe",
            "args": ["tickers.BTCUSDT"]
        })
            .to_string();

        debug!(
            "Sending subscription message: {}",
            subscription_message_json_string
        );

        let req_id_for_verification = req_id.clone();
        let endpoint_name_for_verification = self.endpoint_name.clone();
        let broadcast_receiver = self.broadcaster.resubscribe();

        if let Err(e) = self
            .ws_sender
            .send(Message::Text(subscription_message_json_string))
            .await
        {
            error!("Failed to send subscription: {:?}", e);
        }

        // let subscription_status_sender_clone = self.subscription_status_sender.clone();
        // spawn(async move {
        //     verify_subscription(
        //         broadcast_receiver,
        //         req_id_for_verification,
        //         endpoint_name_for_verification,
        //         subscription_status_sender_clone,
        //     )
        //         .await;
        // });

        info!(
            "subscription sent to {} with req_id {}",
            self.endpoint_name, req_id
        );
    }
}

pub async fn subscription_manager(
    mut subscription_request_receiver: mpsc::Receiver<SubscriptionMessage>,
    broadcaster: Receiver<BroadcastMessage>,
    internalbroadcaster: Sender<Status>,
) {
    while let Some(start_message) = subscription_request_receiver.recv().await {
        let subscription_task = SubscriptionTask {
            endpoint_name: start_message.endpoint_name,
            ws_sender: start_message.ws_sender,
            broadcaster: broadcaster.resubscribe(),
            subscription_status_sender: internalbroadcaster.clone(),
        };
        spawn(subscription_task.start_subscribing());
    }
}
