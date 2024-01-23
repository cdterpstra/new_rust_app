use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast};
use crate::websocket_manager::MyMessage;
use tungstenite::{Message as WebSocketMessage, Message};
use threadpool::ThreadPool;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeData {
    #[serde(rename = "T")]
    timestamp: i64,   // Timestamp van de trade
    #[serde(rename = "s")]
    symbol: String,   // Symboolnaam
    #[serde(rename = "S")]
    side: String,     // Side van de taker (Buy, Sell)
    #[serde(rename = "v")]
    volume: String,   // Trade size als string
    #[serde(rename = "p")]
    price: String,    // Trade price als string
    #[serde(rename = "L")]
    tick_direction: String, // Richting van prijsverandering
    #[serde(rename = "i")]
    trade_id: String, // Trade ID
    #[serde(rename = "BT")]
    block_trade: bool, // Block trade order of niet
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedTradeData {
    symbol: String,
    timestamp: i64,
    open_price: f64,
    close_price: f64,
    highest_price: f64,
    lowest_price: f64,
    average_price: f64,
    total_trades: usize,
    buy_volume: f64,
    sell_volume: f64,
    block_trade: bool,
}





async fn analyze_trading_pair(trading_symbol: String, msg: MyMessage) {
    println!("Analyzing: {:?} for symbol: {}", msg, trading_symbol);
    // Rekenintensieve logica...
}

const MAX_QUEUE_SIZE: usize = 60000;

pub async fn process_messages(mut receiver: broadcast::Receiver<MyMessage>) {
    let active_symbols = Arc::new(Mutex::new(HashMap::<String, bool>::new()));
    let symbol_queues = Arc::new(Mutex::new(HashMap::<String, VecDeque<AggregatedTradeData>>::new()));
    let pool = ThreadPool::new(4);
    debug!("Starting message processing loop");

    loop {
        match receiver.recv().await {
            Ok(my_msg) => {
                if let WebSocketMessage::Text(text) = &my_msg.message {
                    debug!("Received text message: {}", text);

                    if let Ok(parsed_message) = serde_json::from_str::<Value>(text) {
                        debug!("Parsed message successfully");
                        if let Some(topic) = parsed_message["topic"].as_str() {
                            if topic.starts_with("publicTrade") {
                                if let Some(data) = parsed_message["data"].as_array() {
                                    let mut aggregation_map = HashMap::new();

                                    for entry in data {
                                        match serde_json::from_value::<TradeData>(entry.clone()) {
                                            Ok(trade) => {
                                                let volume: f64 = trade.volume.parse().unwrap_or(0.0);
                                                let price: f64 = trade.price.parse().unwrap_or(0.0);

                                                let timestamp_sec = trade.timestamp / 1000;
                                                let key = (trade.symbol.clone(), timestamp_sec);

                                                let agg_data = aggregation_map.entry(key.clone()).or_insert_with(|| AggregatedTradeData {
                                                    symbol: trade.symbol.clone(),
                                                    timestamp: timestamp_sec,
                                                    open_price: price,
                                                    close_price: price,
                                                    highest_price: price,
                                                    lowest_price: price,
                                                    average_price: 0.0,
                                                    total_trades: 0,
                                                    buy_volume: 0.0,
                                                    sell_volume: 0.0,
                                                    block_trade: trade.block_trade,
                                                });

                                                if agg_data.total_trades == 0 {
                                                    agg_data.open_price = price;
                                                }

                                                agg_data.close_price = price;
                                                agg_data.highest_price = agg_data.highest_price.max(price);
                                                agg_data.lowest_price = agg_data.lowest_price.min(price);
                                                agg_data.total_trades += 1;
                                                agg_data.average_price = ((agg_data.average_price * (agg_data.total_trades as f64 - 1.0)) + price) / agg_data.total_trades as f64;
                                                agg_data.block_trade |= trade.block_trade;

                                                if trade.side == "Buy" {
                                                    agg_data.buy_volume += volume;
                                                } else if trade.side == "Sell" {
                                                    agg_data.sell_volume += volume;
                                                }
                                                debug!("Aggregated data for key {:?}: {:?}", key, agg_data);
                                            },
                                            Err(e) => {
                                                debug!("Failed to deserialize trade: {:?}", e);
                                            },
                                        }
                                    }

                                    for (key, agg_data) in aggregation_map.iter() {
                                        let symbol_name = key.0.clone();
                                        let mut queues = symbol_queues.lock().unwrap();
                                        let queue = queues.entry(symbol_name.clone()).or_insert_with(VecDeque::new);

                                        let mut updated = false;
                                        for existing_data in queue.iter_mut() {
                                            if existing_data.timestamp == agg_data.timestamp {
                                                debug!("Updating existing data for symbol: {}, timestamp: {}", symbol_name, existing_data.timestamp);
                                                existing_data.buy_volume += agg_data.buy_volume;
                                                existing_data.sell_volume += agg_data.sell_volume;
                                                updated = true;
                                                break;
                                            }
                                        }

                                        if !updated {
                                            debug!("Adding new data {:?} for symbol: {}, timestamp: {}", agg_data, symbol_name, agg_data.timestamp);
                                            queue.push_back(agg_data.clone());
                                        }

                                        if queue.len() > MAX_QUEUE_SIZE {
                                            debug!("Queue is full, popping front for symbol: {}", symbol_name);
                                            queue.pop_front();
                                        }
                                        debug!("Queue size for symbol {}: {}", symbol_name, queue.len());
                                    }


                                    let symbol_names: Vec<String> = aggregation_map.keys().map(|k| k.0.clone()).collect();

                                    for symbol_name in symbol_names {
                                        let already_active = {
                                            let mut active_symbols = active_symbols.lock().unwrap();
                                            if !active_symbols.contains_key(&symbol_name) {
                                                debug!("Activating symbol: {}", symbol_name);
                                                active_symbols.insert(symbol_name.clone(), true);
                                                false
                                            } else {
                                                debug!("Symbol already active: {}", symbol_name);
                                                true
                                            }
                                        };

                                        if !already_active {
                                            let my_msg_clone = my_msg.clone();
                                            let active_symbols_clone = active_symbols.clone();

                                            debug!("Starting analysis for symbol: {}", symbol_name);
                                            pool.execute(move || {
                                                let rt = tokio::runtime::Runtime::new().unwrap();
                                                rt.block_on(analyze_trading_pair(symbol_name.clone(), my_msg_clone)); // Gebruik de gekloonde symbol_name

                                                let mut active_symbols = active_symbols_clone.lock().unwrap();
                                                active_symbols.remove(&symbol_name);
                                                debug!("Analysis complete, symbol deactivated: {}", symbol_name);
                                            });
                                        } else {
                                            // Optioneel: voeg logica toe voor het geval het symbool al actief is
                                            debug!("Skipping analysis for already active symbol: {}", symbol_name);
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
