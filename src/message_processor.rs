use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Duration}; // Import SystemTime en UNIX_EPOCH
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, Value};
use tokio::sync::broadcast;
use crate::websocket_manager::MyMessage;
use tungstenite::Message as WebSocketMessage;
use tokio::time;
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

// Uitgebreide struct voor geaggregeerde data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AggregatedData {
    start: i64,
    end: i64,
    open: String,
    close: String,
    high: String,
    low: String,
    volume: String,
    turnover: String,
    lasttimestamp: i64,
    trade_count: usize,  // Aantal trades in de aggregatieperiode
    weighted_average_price: f64,  // Gewogen gemiddelde prijs
    dominant_side: String,  // Dominante zijde (Buy of Sell)
    max_tick_direction: Option<String>,  // Maximale tick-richting
    min_tick_direction: Option<String>,  // Minimale tick-richting
    block_trade_count: usize,  // Aantal block trades
    bucket_start: i64,  // Starttijd van de bucket (op basis van systeemklok)
    bucket_end: i64,    // Eindtijd van de bucket (op basis van systeemklok)
    average_delay: i64,  // Nieuw: Gemiddelde vertragingstijd
}

pub async fn process_messages(
    mut receiver: broadcast::Receiver<MyMessage>,
) {
    // Eerste buckets om trades tijdelijk op te slaan per symbool
    let buckets: Arc<DashMap<String, Vec<TradeData>>> = Arc::new(DashMap::new());

    // Tweede buckets voor verdere analyse per symbool
    let analyze_buckets: Arc<DashMap<String, Vec<AggregatedData>>> = Arc::new(DashMap::new());

    // Interval voor periodieke verwerking
    let mut interval = time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            // Verplaats en aggregeer gegevens van de eerste naar de tweede bucket elke seconde
            _ = interval.tick() => {
                let max_age = Duration::from_secs(200 * 3600); // 200 uur in seconden

                // Bepaal de start en eindtijd van de bucket
                let bucket_start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
                let bucket_end = bucket_start + 1000;  // Bucket eindigt 1 seconde later

                for mut entry in buckets.iter_mut() {
                    let symbol = entry.key().clone();
                    let trades = entry.value_mut();

                    if !trades.is_empty() {
                        let start_time = trades.first().unwrap().timestamp;
                        let end_time = trades.last().unwrap().timestamp;

                        // Bereken de gemiddelde vertraging
                        let start_delay = bucket_start - start_time;
                        let end_delay = bucket_end - end_time;
                        let average_delay = (start_delay + end_delay) / 2;

                        // Initialiseer aggregatievariabelen
                        let open = trades.first().unwrap().price.clone();
                        let close = trades.last().unwrap().price.clone();
                        let high = trades.iter().map(|t| &t.price).max().unwrap().clone();
                        let low = trades.iter().map(|t| &t.price).min().unwrap().clone();
                        let volume: f64 = trades.iter().map(|t| t.volume.parse::<f64>().unwrap()).sum();
                        let turnover: f64 = trades.iter().map(|t| t.price.parse::<f64>().unwrap() * t.volume.parse::<f64>().unwrap()).sum();

                        // Nieuwe aggregaties
                        let trade_count = trades.len();
                        let weighted_sum: f64 = trades.iter().map(|t| t.price.parse::<f64>().unwrap() * t.volume.parse::<f64>().unwrap()).sum();
                        let weighted_average_price = if volume > 0.0 {
                            weighted_sum / volume
                        } else {
                            0.0
                        };
                        let buy_volume: f64 = trades.iter().filter(|t| t.side == "Buy").map(|t| t.volume.parse::<f64>().unwrap()).sum();
                        let sell_volume: f64 = trades.iter().filter(|t| t.side == "Sell").map(|t| t.volume.parse::<f64>().unwrap()).sum();
                        let dominant_side = if buy_volume > sell_volume { "Buy".to_string() } else { "Sell".to_string() };
                        let max_tick_direction = trades.iter().filter_map(|t| t.tick_direction.clone()).max();
                        let min_tick_direction = trades.iter().filter_map(|t| t.tick_direction.clone()).min();
                        let block_trade_count: usize = trades.iter().filter(|t| t.block_trade).count();

                        // Maak geaggregeerde data aan
                        let aggregated_data = AggregatedData {
                            start: start_time,
                            end: end_time,
                            open,
                            close,
                            high,
                            low,
                            volume: volume.to_string(),
                            turnover: turnover.to_string(),
                            lasttimestamp: end_time,
                            trade_count,  // Nieuw
                            weighted_average_price,  // Nieuw
                            dominant_side,  // Nieuw
                            max_tick_direction,  // Nieuw
                            min_tick_direction,  // Nieuw
                            block_trade_count,  // Nieuw
                            bucket_start,  // Starttijd van de bucket
                            bucket_end,    // Eindtijd van de bucket
                            average_delay, // Nieuw: Gemiddelde vertragingstijd
                        };

                        // Voeg de geaggregeerde data toe aan de analyze bucket
                        let mut analyze_bucket = analyze_buckets.entry(symbol.clone()).or_insert_with(Vec::new);
                        analyze_bucket.push(aggregated_data);

                        // Bereken de grootte van de tweede bucket in bytes
                        let analyze_bucket_size_bytes: usize = analyze_bucket.iter().map(|data| size_of_val(data)).sum();
                        let analyze_bucket_size_mb = analyze_bucket_size_bytes as f64 / (1024.0 * 1024.0);

                        // Log de grootte van de tweede bucket
                        info!("Second bucket size for symbol {}: {:.2} MB", symbol, analyze_bucket_size_mb);

                        // Verwijder data ouder dan 200 uur
                        analyze_bucket.retain(|data| {
                            let data_time = UNIX_EPOCH + Duration::from_secs(data.lasttimestamp as u64);
                            let elapsed = SystemTime::now().duration_since(data_time).unwrap_or_else(|_| Duration::from_secs(0));
                            elapsed <= max_age
                        });

                        // Leeg de eerste bucket na verplaatsing
                        trades.clear();

                        // Toon de laatste 5 regels in een tabel
                        let table = analyze_bucket.iter().rev().take(5)
                            .map(|data| format!(
                                "{:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15}",
                                data.start,
                                data.end,
                                data.open,
                                data.close,
                                data.high,
                                data.low,
                                data.volume,
                                data.turnover,
                                data.lasttimestamp,
                                data.trade_count,
                                data.weighted_average_price,
                                data.dominant_side,
                                data.bucket_start,
                                data.bucket_end,
                                data.average_delay
                            ))
                            .collect::<Vec<String>>()
                            .join("\n");

                        info!(
                            "Last 5 entries for symbol {}:\n{:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15} {:<15}\n{}",
                            symbol,
                            "Start", "End", "Open", "Close", "High", "Low", "Volume", "Turnover", "Lasttimestamp", "TradeCount", "WeightedAvgPrice", "DominantSide", "BucketStart", "BucketEnd", "AvgDelay",
                            table
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
