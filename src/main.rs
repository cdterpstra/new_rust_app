// main.rs

// Module Imports
mod common;
mod listener;
mod ping_manager;
mod websocket_manager;
mod subscription_manager;

// External Library Imports
use crate::common::{BroadcastMessage, Status, StartPingMessage, SubscriptionMessage};
use crate::ping_manager::ping_manager;
use log::debug;
use tokio::signal;
use tokio::sync::{broadcast, mpsc};
use crate::subscription_manager::subscription_manager;

// Local Module Imports
use crate::websocket_manager::websocket_manager;

#[tokio::main]
async fn main() {
    // Initialize the logger
    env_logger::init();
    debug!("Application started");

    // Configuration
    let base_url = "wss://stream-testnet.bybit.com/v5/";
    let endpoints = vec![
        "public/spot",
        // "public/linear",
        // "public/inverse",
        // "public/option",
        // "private",
    ];

    // Setup Broadcast channel
    let (broadcaster, _) = broadcast::channel::<BroadcastMessage>(16); // Only the broadcaster is used later

    let (ping_request_sender, ping_request_receiver) = mpsc::channel::<StartPingMessage>(100);
    let (subscription_request_sender, subscription_request_receiver) = mpsc::channel::<SubscriptionMessage>(100);
    let (internalbroadcaster, _) = broadcast::channel::<Status>(100);

    // Start the ping manager
    tokio::spawn(ping_manager(
        ping_request_receiver,
        broadcaster.subscribe(),
        internalbroadcaster.clone(),
    ));

    // Start the subscription manager
    tokio::spawn(subscription_manager(
        subscription_request_receiver,
        broadcaster.subscribe(),
        internalbroadcaster.clone(),
    ));

    // Setup listener for incoming messages
    let listener_receiver = broadcaster.subscribe(); // Receiver for the listener
    tokio::spawn(listener::listen_for_messages(listener_receiver));

    websocket_manager(base_url, endpoints, broadcaster, ping_request_sender, internalbroadcaster.subscribe(), subscription_request_sender)
    .await;

    // Await until signal for shutdown is received
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
