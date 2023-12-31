// main.rs

mod websocket_manager; // Import the module containing websocket_manager
mod write_task;
mod broadcast_task;
mod common;
mod subscription_manager;
mod authentication;
mod ping_manager;

use std::collections::HashMap;
use std::sync::Arc;
use crate::websocket_manager::Connection;
use tokio::sync::mpsc::channel;
use tokio::sync::{broadcast, mpsc, RwLock};
use log::debug;
use tokio::signal;
use crate::common::ConnectionMessage;


#[tokio::main]
async fn main() {
    env_logger::init(); // Initialize the logger

    debug!("Application started"); // Log application start

    // Define base URL and endpoints
    let base_url = "wss://stream-testnet.bybit.com/v5/";
    let endpoints = vec![
        "public/spot",
        // "public/linear",
        // "public/inverse",
        // "public/option",
        // "private",
    ];

    let (_, global_receiver) = channel::<ConnectionMessage>(100);
    let (broadcaster, _) = broadcast::channel::<String>(100);
    let (signal_sender, signal_receiver) = mpsc::channel::<String>(100);
    let (write_sender, _) = tokio::sync::mpsc::channel::<String>(100);

    let connection_map: Arc<RwLock<HashMap<u128, Connection>>> = Arc::new(RwLock::new(HashMap::new()));
    debug!("Initialized empty connection_map.");

    debug!("Starting PingManager.");
    let ping_manager = ping_manager::PingManager::new(connection_map.clone(), write_sender);
    ping_manager.start(signal_receiver).await;

    debug!("Starting websocket manager.");
    websocket_manager::websocket_manager(
        base_url,
        endpoints,
        global_receiver,
        broadcaster,
        signal_sender,
        connection_map.clone()
    ).await;

    debug!("Websocket manager started.");

    debug!("Waiting for CTRL+C signal.");
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
    debug!("CTRL+C received, exiting...");

    debug!("Application shutdown");
}
