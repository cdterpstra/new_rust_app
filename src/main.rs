// main.rs

// Module Imports
mod listener;
mod ping_manager;
mod websocket_manager;
mod common;

// External Library Imports
use log::debug;
use tokio::signal;
use tokio::sync::{broadcast, mpsc};
use crate::ping_manager::ping_manager;
use crate::common::{StartPingMessage, BroadcastMessage, PongStatus};

// Local Module Imports
use crate::websocket_manager::{websocket_manager};

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
    let (ping_request_sender, ping_request_receiver) = mpsc::channel::<StartPingMessage>(100);
    let (ping_status_sender, ping_status_receiver) = mpsc::channel::<PongStatus>(100);

    // Start the ping manager
    tokio::spawn(ping_manager(ping_request_receiver, broadcaster.clone(), ping_status_sender));

    // Setup listener for incoming messages
    let listener_receiver = broadcaster.subscribe(); // Receiver for the listener
    tokio::spawn(listener::listen_for_messages(listener_receiver));


    websocket_manager(base_url, endpoints, broadcaster, ping_request_sender).await;

    // Await until signal for shutdown is received
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
