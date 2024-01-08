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
use ::config::File;
use ::config::Config;
use crate::config::AppConfig;
use crate::websocket_manager::websocket_manager;

// ====================
// Application Entry Point
// ====================
#[tokio::main]
async fn main() {
    // Initialize logger
    env_logger::init();
    debug!("Application started");

    // Set up configuration
    let settings = Config::builder()
        .add_source(File::with_name("config/default"))
        .build()
        .unwrap();

    let app_config: AppConfig = settings.try_deserialize().unwrap();

    // Rest of your application logic remains the same
    websocket_manager(
        &app_config.base_url,
        &app_config.endpoints,
    ).await;


    // Wait for CTRL+C signal for graceful shutdown
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
