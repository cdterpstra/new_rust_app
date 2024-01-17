use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use log::debug;
use serde_json::Value;
use tokio::sync::{broadcast};
use crate::websocket_manager::MyMessage;
use tungstenite::Message as WebSocketMessage;
use threadpool::ThreadPool;

async fn analyze_trading_pair(trading_symbol: String, msg: MyMessage) {
    println!("Analyzing: {:?} for symbol: {}", msg, trading_symbol);
    // Rekenintensieve logica...
}


pub async fn process_messages(mut receiver: broadcast::Receiver<MyMessage>) {
    let active_symbols = Arc::new(Mutex::new(HashMap::<String, bool>::new()));
    let pool = ThreadPool::new(4);
    // Creëer een threadpool met 4 threads
    debug!("Starting message processing loop");
    loop {
        match receiver.recv().await {
            Ok(my_msg) => {
                if let WebSocketMessage::Text(text) = &my_msg.message {
                    debug!("Received text message: {}", text);
                    if let Ok(parsed_message) = serde_json::from_str::<Value>(text) {
                        if let Some(topic) = parsed_message["topic"].as_str() {
                            if topic.starts_with("publicTrade") {
                                if let Some(first_entry) = parsed_message["data"].as_array().and_then(|arr| arr.first()) {
                                    if let Some(symbol) = first_entry["s"].as_str() {
                                        let symbol_name = symbol.to_string();
                                        println!("Received 'publicTrade' for symbol: {}", symbol_name);

                                        let already_active = {
                                            let mut active_symbols = active_symbols.lock().unwrap();
                                            if !active_symbols.contains_key(&symbol_name) {
                                                active_symbols.insert(symbol_name.clone(), true);
                                                false
                                            } else {
                                                true
                                            }
                                        };

                                        if !already_active {
                                            let my_msg_clone = my_msg.clone();
                                            let active_symbols_clone = active_symbols.clone();

                                            pool.execute(move || {
                                                let rt = tokio::runtime::Runtime::new().unwrap();
                                                rt.block_on(analyze_trading_pair(symbol_name.clone(), my_msg_clone)); // Gebruik de gekloonde symbol_name

                                                let mut active_symbols = active_symbols_clone.lock().unwrap();
                                                active_symbols.remove(&symbol_name);
                                            });
                                        } else {
                                            // Optioneel: implementeer logica voor het in de wachtrij zetten van berichten
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        eprintln!("Failed to parse message as JSON");
                    }
                } else {
                    eprintln!("Received a non-text WebSocket message");
                }
            },
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
            }
        }
    }
}