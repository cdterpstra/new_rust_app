use std::sync::Arc;
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, Value};
use tokio::sync::broadcast;
use crate::websocket_manager::MyMessage;
use tungstenite::Message as WebSocketMessage;
use tokio::time::{self, Duration};
use dashmap::DashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeData {
    #[serde(rename = "T")]
    pub timestamp: i64,   // Timestamp van de trade
    #[serde(rename = "s")]
    pub symbol: String,   // Symboolnaam
    #[serde(rename = "S")]
    pub side: String,     // Side van de taker (Buy, Sell)
    #[serde(rename = "v")]
    pub volume: String,   // Trade size als string
    #[serde(rename = "p")]
    pub price: String,    // Trade price als string
    #[serde(rename = "L", default)]
    pub tick_direction: Option<String>, // Richting van prijsverandering (kan ontbreken)
    #[serde(rename = "i")]
    pub trade_id: String, // Trade ID
    #[serde(rename = "BT")]
    pub block_trade: bool, // Block trade order of niet
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TradeMessage {
    pub topic: String,
    pub ts: i64,
    pub r#type: String,
    pub data: Vec<TradeData>,
}

pub async fn process_messages(
    mut receiver: broadcast::Receiver<MyMessage>,
) {
    // Eerste buckets om trades tijdelijk op te slaan per symbool
    let buckets: Arc<DashMap<String, Vec<TradeData>>> = Arc::new(DashMap::new());

    // Tweede buckets voor verdere analyse per symbool
    let analyze_buckets: Arc<DashMap<String, Vec<TradeData>>> = Arc::new(DashMap::new());

    // Interval voor periodieke verwerking
    let mut interval = time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            // Verplaats gegevens van de eerste naar de tweede bucket elke seconde
            _ = interval.tick() => {
                for mut entry in buckets.iter_mut() {
                    let symbol = entry.key().clone();
                    let trades = entry.value_mut();

                    let num_trades = trades.len(); // Aantal trades dat wordt verplaatst
                    if num_trades > 0 {
                        let mut analyze_bucket = analyze_buckets.entry(symbol.clone()).or_insert_with(Vec::new);
                        analyze_bucket.extend(trades.drain(..)); // Verplaats alle data en leeg de eerste bucket

                        let total_in_analyze_bucket = analyze_bucket.len(); // Aantal trades in de analyze bucket na verplaatsing

                        info!(
                            "Moved {} trades for symbol {} to analyze bucket '{}'. Analyze bucket now contains {} trades.",
                            num_trades, symbol, symbol, total_in_analyze_bucket
                        );
                    }
                }
            },

            // Ontvang en verwerk inkomende berichten
            result = receiver.recv() => {
                match result {
                    Ok(my_msg) => {
                        tokio::spawn({
                            let buckets = Arc::clone(&buckets);
                            async move {
                                trace!(
                                    "Received Message with timestamp {} from {}: {:?}",
                                    my_msg.receivedat, my_msg.endpoint_name, my_msg.message
                                );

                                if let WebSocketMessage::Text(ref text) = my_msg.message {
                                    match from_str::<Value>(text) {
                                        Ok(json_value) => {
                                            if let Some(topic) = json_value["topic"].as_str() {
                                                if topic.starts_with("publicTrade") {
                                                    trace!("Topic is '{}', processing message.", topic);

                                                    // Deserialiseer naar TradeMessage
                                                    if let Ok(trade_message) = from_str::<TradeMessage>(text) {
                                                        for trade_data in trade_message.data {
                                                            trace!("Received trade data: {:?}", trade_data);

                                                            // Voeg de trade data toe aan de juiste bucket voor het symbool
                                                            buckets.entry(trade_data.symbol.clone()).or_insert_with(Vec::new).push(trade_data.clone());
                                                        }
                                                    } else {
                                                        error!("Failed to deserialize trade data: {:?}", text);
                                                    }
                                                } else if topic.starts_with("tickers") {
                                                    trace!("Topic starts with 'tickers', processing message.");
                                                    // Hier kun je logica toevoegen om ticker berichten te verwerken
                                                }
                                            }
                                        }
                                        Err(e) => error!("Failed to parse message as JSON: {}", e),
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to receive message: {:?}", e);
                    }
                }
            }
        }
    }
}
