// main.rs

// Module Imports
mod listener;
mod ping_manager;
mod websocket_manager;

// External Library Imports
use log::debug;
use tokio::signal;
use tokio::sync::{broadcast, mpsc};
use crate::ping_manager::ping_manager;

// Local Module Imports
use crate::websocket_manager::{websocket_manager, BroadcastMessage, StartPingMessage};

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

// Create mpsc channel for start ping messages
    let (internal_sender, internal_receiver) = mpsc::channel::<StartPingMessage>(100);

    // Start the ping manager
    tokio::spawn(ping_manager(internal_receiver));

    // Setup listener for incoming messages
    let listener_receiver = broadcaster.subscribe(); // Receiver for the listener
    tokio::spawn(listener::listen_for_messages(listener_receiver));


    websocket_manager(base_url, endpoints, broadcaster, internal_sender).await;

    // Await until signal for shutdown is received
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
