use crate::db_connection_manager::PgPool;
use crate::schema::crypto::tickers;
use crate::websocket_manager::MyMessage;
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use diesel::prelude::*;
use log::{debug, error, info, trace};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{from_str, Value};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tungstenite::Message as WebSocketMessage;
use std::str::FromStr;
use colored::Colorize;
use diesel::debug_query;
use diesel::pg::Pg;

fn deserialize_optional_string_timestamp<'de, D>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let option = Option::<String>::deserialize(deserializer)?;
    match option {
        Some(s) => {
            let micros = i64::from_str(&s).map_err(serde::de::Error::custom)?;
            let naive_datetime = NaiveDateTime::from_timestamp_micros(micros / 1000)
                .ok_or_else(|| serde::de::Error::custom("Invalid timestamp"))?;
            Ok(Some(Utc.from_utc_datetime(&naive_datetime)))
        }
        None => Ok(None),
    }
}


#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct MessageData {
    pub topic: String,
    #[serde(rename = "type")]
    pub datatype: String,
    pub data: Data,
    pub cs: i64,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[allow(non_snake_case)]
pub struct Data {
    pub symbol: String,
    pub tickDirection: Option<String>,
    pub price24hPcnt: Option<BigDecimal>,
    pub lastPrice: Option<BigDecimal>,
    pub prevPrice24h: Option<BigDecimal>,
    pub highPrice24h: Option<BigDecimal>,
    pub lowPrice24h: Option<BigDecimal>,
    pub prevPrice1h: Option<BigDecimal>,
    pub markPrice: Option<BigDecimal>,
    pub indexPrice: Option<BigDecimal>,
    pub openInterest: Option<BigDecimal>,
    pub openInterestValue: Option<BigDecimal>,
    pub turnover24h: Option<BigDecimal>,
    pub volume24h: Option<BigDecimal>,
    #[serde(deserialize_with = "deserialize_optional_string_timestamp", default)]
    pub nextFundingTime: Option<DateTime<Utc>>,
    pub fundingRate: Option<BigDecimal>,
    pub bid1Price: Option<BigDecimal>,
    pub bid1Size: Option<BigDecimal>,
    pub ask1Price: Option<BigDecimal>,
    pub ask1Size: Option<BigDecimal>,
}

#[derive(Insertable, Queryable, Debug)]
#[diesel(table_name = tickers)]
#[allow(non_snake_case)]
struct NewMessage {
    datatype: String,
    symbol: String,
    tickdirection: Option<String>,
    price24hpcnt: Option<BigDecimal>,
    lastprice: Option<BigDecimal>,
    prevprice24h: Option<BigDecimal>,
    highprice24h: Option<BigDecimal>,
    lowprice24h: Option<BigDecimal>,
    prevprice1h: Option<BigDecimal>,
    markprice: Option<BigDecimal>,
    indexprice: Option<BigDecimal>,
    openinterest: Option<BigDecimal>,
    openinterestvalue: Option<BigDecimal>,
    turnover24h: Option<BigDecimal>,
    volume24h: Option<BigDecimal>,
    nextfundingtime: Option<NaiveDateTime>,
    fundingrate: Option<BigDecimal>,
    bid1price: Option<BigDecimal>,
    bid1size: Option<BigDecimal>,
    ask1price: Option<BigDecimal>,
    ask1size: Option<BigDecimal>,
    cs: Option<i64>,
    ts: Option<NaiveDateTime>,
    endpoint: Option<String>,
    receivedat: NaiveDateTime,
}

async fn insert_messages_into_db(
    new_messages: Vec<NewMessage>,
    conn: &mut PgConnection,
) -> Result<(), diesel::result::Error> {
    if new_messages.is_empty() {
        return Ok(());
    }

    info!("Inserting {} messages into the database", new_messages.len());

    // Maak de insert statement
    let insert_statement = diesel::insert_into(tickers::table).values(&new_messages);

    // Print de debug versie van de SQL-query
    trace!("SQL: {}", debug_query::<Pg, _>(&insert_statement).to_string());

    // Voer de insert statement uit
    match insert_statement.execute(conn) {
        Ok(_) => trace!("Batch insertion successful"),
        Err(e) => error!("{} {}", "Error inserting batch:".red(), e.to_string().red()),
    }

    Ok(())
}

pub async fn insert_into_db(mut receiver: broadcast::Receiver<MyMessage>, pool: PgPool) {
    let mut buffer = Vec::new();
    let mut interval = interval(Duration::from_millis(100 ));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    let mut conn = pool.get().expect("Failed to get database connection from pool");
                    let _ = insert_messages_into_db(buffer.drain(..).collect(), &mut conn).await;
                }
            }
            msg = receiver.recv() => {
                match msg {
                    Ok(my_msg) => {
                        trace!(
                            "Received Message with timestamp {} from {}: {:?}",
                            my_msg.receivedat, my_msg.endpoint_name, my_msg.message
                        );

                        if let WebSocketMessage::Text(ref text) = my_msg.message {
                            match serde_json::from_str::<Value>(text) {
                                Ok(json_value) => {
                                    if let Some(topic) = json_value["topic"].as_str() {
                                        if topic.starts_with("tickers") {
                                            trace!("Topic starts with 'tickers', processing message.");

                                            if let Ok(parsed_message) = from_str::<MessageData>(text) {
                                                let received_at_time = NaiveDateTime::from_timestamp_micros(my_msg.receivedat)
                                                    .expect("Invalid timestamp");
                                                let new_message = NewMessage {
                                                    datatype: parsed_message.datatype,
                                                    symbol: parsed_message.data.symbol,
                                                    tickdirection: parsed_message.data.tickDirection,
                                                    price24hpcnt: parsed_message.data.price24hPcnt,
                                                    lastprice: parsed_message.data.lastPrice,
                                                    prevprice24h: parsed_message.data.prevPrice24h,
                                                    highprice24h: parsed_message.data.highPrice24h,
                                                    lowprice24h: parsed_message.data.lowPrice24h,
                                                    prevprice1h: parsed_message.data.prevPrice1h,
                                                    markprice: parsed_message.data.markPrice,
                                                    indexprice: parsed_message.data.indexPrice,
                                                    openinterest: parsed_message.data.openInterest,
                                                    openinterestvalue: parsed_message.data.openInterestValue,
                                                    turnover24h: parsed_message.data.turnover24h,
                                                    volume24h: parsed_message.data.volume24h,
                                                    nextfundingtime: parsed_message.data.nextFundingTime.map(|dt| dt.naive_utc()),
                                                    fundingrate: parsed_message.data.fundingRate,
                                                    bid1price: parsed_message.data.bid1Price,
                                                    bid1size: parsed_message.data.bid1Size,
                                                    ask1price: parsed_message.data.ask1Price,
                                                    ask1size: parsed_message.data.ask1Size,
                                                    cs: Some(parsed_message.cs),
                                                    ts: Some(parsed_message.ts.naive_utc()),
                                                    endpoint: Some(my_msg.endpoint_name.clone()),
                                                    receivedat: received_at_time,
                                                };

                                                buffer.push(new_message);
                                            } else {
                                                error!("Failed to parse into MessageData structure");
                                            }
                                        } else {
                                            trace!("Received message with a different topic: {}", topic);
                                        }
                                    } else {
                                        error!("JSON does not contain 'topic' field");
                                    }
                                }
                                Err(e) => {
                                    error!("Error parsing message JSON: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(number)) => {
                        error!("{} {} {}", "Missed".red(), number.to_string().red(), "messages due to lagging receiver".red());
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        error!("Broadcast channel closed");
                        break;
                    }
                }
            }
        }
    }
}
