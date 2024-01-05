// main.rs

// ====================
// Module Imports
// ====================
mod common;
mod listener;
mod ping_manager;
mod websocket_manager;
mod subscription_manager;

// ====================
// External Library Imports
// ====================
use crate::common::{BroadcastMessage, Status, StartTaskMessage};
use crate::ping_manager::ping_manager;
use crate::subscription_manager::subscription_manager;
use crate::websocket_manager::websocket_manager;
use log::debug;
use tokio::signal;
use tokio::sync::{broadcast, mpsc};

// ====================
// Application Entry Point
// ====================
#[tokio::main]
async fn main() {
    // Initialize the logger for application-wide logging
    env_logger::init();
    debug!("Application started");

    // Configuration for the WebSocket connections
    let base_url = "wss://stream-testnet.bybit.com/v5/";
    let endpoints = vec![
        "public/spot",
        "public/linear",
        "public/inverse",
        "public/option",
        "private",
    ];

    // Setup Broadcast channel to disseminate messages to multiple listeners
    let (broadcaster, _) = broadcast::channel::<BroadcastMessage>(16);

    // Setup channels for ping and subscription requests and internal broadcasting
    let (ping_request_sender, ping_request_receiver) = mpsc::channel::<StartTaskMessage>(100);
    let (subscription_request_sender, subscription_request_receiver) = mpsc::channel::<StartTaskMessage>(100);
    let (internal_broadcaster, _) = broadcast::channel::<Status>(100);

    // ====================
    // Service Initialization Section
    // ====================

    // Start the ping manager to handle ping messages
    tokio::spawn(ping_manager(
        ping_request_receiver,
        broadcaster.subscribe(),
        internal_broadcaster.clone(),
    ));

    // Start the subscription manager to handle subscription messages
    tokio::spawn(subscription_manager(
        subscription_request_receiver,
        broadcaster.subscribe(),
        internal_broadcaster.clone(),
    ));

    // Start the listener to handle incoming broadcast messages
    let listener_receiver = broadcaster.subscribe(); // Receiver for the listener
    tokio::spawn(listener::listen_for_messages(listener_receiver));

    // Initialize and run the WebSocket manager for handling connections
    websocket_manager(
        base_url,
        endpoints,
        broadcaster,
        ping_request_sender,
        internal_broadcaster.subscribe(),
        subscription_request_sender,
    )
        .await;

    // Wait for CTRL+C signal for graceful shutdown
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
