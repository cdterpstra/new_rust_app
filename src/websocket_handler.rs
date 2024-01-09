use futures_util::stream::SplitStream;
use futures_util::StreamExt;
use log::{debug, error};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tungstenite::Message;
use crate::websocket_manager::{MyMessage, PongMessage, SubscribeMessage, forward_general_message};

pub async fn handle_websocket_stream<S>(
    mut read: SplitStream<WebSocketStream<S>>,
    uri: &str,
    pong_tx: mpsc::Sender<MyMessage>,
    subscribe_response_tx: mpsc::Sender<MyMessage>,
    general_tx: mpsc::Sender<MyMessage>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    while let Some(message) = read.next().await {
        if let Err(e) = process_message(message, uri, &pong_tx, &subscribe_response_tx, &general_tx).await {
            error!("Error processing ws message: {:?}", e);
            break; // Exit the loop on error
        }
    }
}

async fn process_message(
    message: Result<Message, tungstenite::Error>,
    uri: &str,
    pong_tx: &mpsc::Sender<MyMessage>,
    subscribe_response_tx: &mpsc::Sender<MyMessage>,
    general_tx: &mpsc::Sender<MyMessage>,
) -> Result<(), String> {
    let msg = message.map_err(|e| format!("Error receiving ws message: {:?}", e))?;

    if let Message::Text(text) = msg {
        let value = serde_json::from_str::<Value>(&text)
            .map_err(|e| format!("Failed to parse message: {:?}", e))?;

        match value["op"].as_str() {
            Some("ping") | Some("pong") => handle_pong_message(text, uri, pong_tx).await,
            Some("subscribe") => handle_subscribe_message(text, uri, subscribe_response_tx).await,
            _ => Ok(forward_general_message(text, uri, general_tx).await),
        }
    } else {
        Ok(())
    }
}

async fn handle_pong_message(
    text: String,
    uri: &str,
    pong_tx: &mpsc::Sender<MyMessage>,
) -> Result<(), String> {
    let parsed_msg: PongMessage = serde_json::from_str(&text)
        .expect("Failed to parse message as PongMessage");
    debug!("Parsed message as PongMessage: {:?}", parsed_msg);

    let my_msg = create_my_message(text, uri);
    pong_tx.send(my_msg).await.map_err(|e| format!("Error forwarding to pong handler: {:?}", e))
}

async fn handle_subscribe_message(
    text: String,
    uri: &str,
    subscribe_response_tx: &mpsc::Sender<MyMessage>,
) -> Result<(), String> {
    let _parsed_msg: SubscribeMessage = serde_json::from_str(&text)
        .expect("Failed to parse message as SubscribeMessage");
    debug!("Forwarding subscribe message");

    let my_msg = create_my_message(text, uri);
    subscribe_response_tx.send(my_msg).await.map_err(|e| format!("Error forwarding to subscribe handler: {:?}", e))
}

fn create_my_message(text: String, uri: &str) -> MyMessage {
    MyMessage {
        timestamp: chrono::Utc::now().timestamp_millis() as u128,
        endpoint_name: uri.to_string(),
        message: Message::Text(text), // Repackaging text as Message
    }
}
