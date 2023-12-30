// main.rs

mod websocket_manager; // Import the module containing websocket_manager
mod write_task;
mod connect_to_websocket;
mod broadcast_task;
mod common;

use crate::websocket_manager::{websocket_manager};
use tokio::sync::mpsc::channel;
use tokio::sync::broadcast;
use log::debug; // Import the debug! macro
use tokio::signal; // Import signal handling
use crate::common::ConnectionMessage;

#[tokio::main]
async fn main() {
    env_logger::init(); // Initialize the logger

    debug!("Application started"); // Log application start

    // Define base URL and endpoints
    let base_url = "wss://stream-testnet.bybit.com/v5/";
    let endpoints = vec![
        "public/spot",
        "public/linear",
        "public/inverse",
        "public/option",
        "private",
    ];

    debug!("Base URL and endpoints defined: {}", base_url); // Log the base URL

    // Set up communication channels
    let (global_sender, global_receiver) = channel::<ConnectionMessage>(100); // Adjust size as needed
    let (broadcaster, _) = broadcast::channel::<String>(100); // Adjust size as needed

    debug!("Communication channels set up"); // Log the setup of communication channels

    // Run the websocket manager
    debug!("Starting websocket manager"); // Log before starting the websocket manager
    let ws_manager = websocket_manager(base_url, endpoints, global_receiver, broadcaster);

    // Use tokio::signal to wait for a CTRL+C event
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
        debug!("CTRL+C received, exiting...");
    };

    tokio::select! {
        _ = ws_manager => debug!("Websocket manager exited"),
        _ = ctrl_c => debug!("Program exiting after CTRL+C"),
    }

    debug!("Application shutdown"); // Log application shutdown
}
