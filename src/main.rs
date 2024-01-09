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
mod auth_module;
mod websocket_handler;


// ====================
// External Library Imports
// ====================
use log::{debug, error};
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
    // Set up configuration
    let settings = match Config::builder()
        .add_source(File::with_name("config/default"))
        .build() {
        Ok(settings) => settings,
        Err(e) => {
            error!("Failed to build configuration: {}", e);
            return;
        }
    };

    // At this point, settings is of type Config, not Result
    println!("Config read successfully: {:?}", settings);

    let app_config: AppConfig = match settings.try_deserialize() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to deserialize AppConfig: {}", e);
            return;
        }
    };


    // Rest of your application logic remains the same
    websocket_manager(
        &app_config.base_url,
        &app_config.endpoints,
    ).await;


    // Wait for CTRL+C signal for graceful shutdown
    signal::ctrl_c().await.expect("Failed to listen for CTRL+C");
}
