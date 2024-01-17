use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use log::debug;
use serde_json::Value;
use tokio::sync::{broadcast};
use crate::websocket_manager::MyMessage;
use tungstenite::Message as WebSocketMessage;

async fn analyze_trading_pair(trading_symbol: String, msg: MyMessage) {
    println!("Analyzing: {:?} for symbol: {}", msg, trading_symbol);
    // Rekenintensieve logica...
}


pub async fn process_messages(mut receiver: broadcast::Receiver<MyMessage>) {
    let active_symbols = Arc::new(Mutex::new(HashMap::<String, bool>::new()));
    debug!("Starting message processing loop");
    loop {
        match receiver.recv().await {
            Ok(my_msg) => {
                if let WebSocketMessage::Text(text) = &my_msg.message {
                    debug!("Received text message: {}", text);
                    if let Ok(parsed_message) = serde_json::from_str::<Value>(text) {
                        if let Some(topic) = parsed_message["topic"].as_str() {
                            if topic.starts_with("publicTrade") {
                                let symbol_name = parsed_message["data"]["symbol"].as_str().unwrap_or_default().to_string();

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
                                    let symbol_clone = symbol_name.clone();

                                    thread::spawn(move || {
                                        let rt = tokio::runtime::Runtime::new().unwrap();
                                        rt.block_on(analyze_trading_pair(symbol_clone, my_msg_clone));

                                        // Deze sluiting markeert het einde van de code die in de thread wordt uitgevoerd
                                        let mut active_symbols = active_symbols_clone.lock().unwrap();
                                        active_symbols.remove(&symbol_name);
                                    }); // Deze sluiting markeert het einde van de thread::spawn closure
                                } else {
                                    // Optioneel: implementeer logica voor het in de wachtrij zetten van berichten
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