// main.rs

// ====================
// Module Imports
// ====================
mod common;
mod listener;
mod ping_manager;
mod websocket_manager;
mod subscription_manager;
mod config;

// ====================
// External Library Imports
// ====================
use log::debug;
use tokio::signal;
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


    // ====================
    // Service Initialization Section
    // ====================

    // Initialize and run the WebSocket manager for handling connections
    websocket_manager(
        base_url,
        endpoints,
    )
        .await;

    // Wait for CTRL+C signal for graceful shutdown
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
