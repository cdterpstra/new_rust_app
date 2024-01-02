// main.rs

// Module Imports
mod common;
mod listener;
mod ping_manager;
mod websocket_manager;

// External Library Imports
use log::debug;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::{broadcast, RwLock};

// Local Module Imports
use crate::ping_manager::ping_manager;
use crate::websocket_manager::{websocket_manager, BroadcastMessage};

#[tokio::main]
async fn main() {
    // Initialize the logger
    env_logger::init();
    debug!("Application started");

    // Configuration
    let base_url = "wss://stream-testnet.bybit.com/v5/";
    let endpoints = vec![
        "public/spot",
        "public/linear",
        "public/inverse",
        "public/option",
        "private"
    ];

    // Setup Broadcast channel
    let (broadcaster, _) = broadcast::channel::<BroadcastMessage>(16); // Only the broadcaster is used later

    // Initialize shared connection map
    let connection_map: Arc<RwLock<HashMap<u128, crate::common::WebSocketConnection>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Setup listener for incoming messages
    let listener_receiver = broadcaster.subscribe(); // Receiver for the listener
    tokio::spawn(listener::listen_for_messages(listener_receiver));

    // Background tasks: Ping Manager and Websocket Manager
    let ping_receiver = broadcaster.subscribe(); // Receiver for the ping manager
    tokio::spawn(ping_manager(connection_map.clone(), ping_receiver)); // Pass the receiver here

    websocket_manager(base_url, endpoints, connection_map, broadcaster).await;

    // Await until signal for shutdown is received
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
