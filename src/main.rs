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
use crate::common::{BroadcastMessage};
use log::debug;
use tokio::signal;
use tokio::sync::broadcast;
use crate::websocket_manager::websocket_manager;

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


    // ====================
    // Service Initialization Section
    // ====================




    // Start the listener to handle incoming broadcast messages
    let listener_receiver = broadcaster.subscribe(); // Receiver for the listener
    tokio::spawn(listener::listen_for_messages(listener_receiver));

    // Initialize and run the WebSocket manager for handling connections
    websocket_manager(
        base_url,
        endpoints,
    )
        .await;

    // Wait for CTRL+C signal for graceful shutdown
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
