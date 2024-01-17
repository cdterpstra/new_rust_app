use crate::db_connection_manager::PgPool;
use crate::schema::crypto::tickers;
use crate::websocket_manager::MyMessage;
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use diesel::prelude::*;
use log::{error, info, trace};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{from_str, Value};
use std::str::FromStr;
use tokio::sync::broadcast;
use tungstenite::Message as WebSocketMessage;

fn deserialize_optional_string_timestamp<'de, D>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let option = Option::<String>::deserialize(deserializer)?;
    match option {
        Some(s) => {
            let millis = i64::from_str(&s).map_err(serde::de::Error::custom)?;
            match Utc.timestamp_millis_opt(millis) {
                chrono::LocalResult::Single(timestamp) => Ok(Some(timestamp)),
                _ => Err(serde::de::Error::custom("Invalid timestamp")),
            }
        }
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MessageData {
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

pub async fn insert_message_into_db(
    other_receivedat: i64,
    endpoint_name: String,
    message: MessageData,
    conn: &mut PgConnection, // Change the reference to mutable
) -> Result<(), diesel::result::Error> {
    let message_clone = message.clone();

    let seconds = other_receivedat / 1_000_000_000;
    let nanoseconds = (other_receivedat % 1_000_000_000) as u32;
    let received_at_time =
        NaiveDateTime::from_timestamp_opt(seconds, nanoseconds).expect("Invalid timestamp");

    // Create a NewMessage struct with the data you want to insert
    let new_message = NewMessage {
        // topic: message_clone.topic,
        datatype: message_clone.datatype,
        symbol: message_clone.data.symbol,
        tickdirection: message_clone.data.tickDirection,
        price24hpcnt: message_clone.data.price24hPcnt,
        lastprice: message_clone.data.lastPrice,
        prevprice24h: message_clone.data.prevPrice24h,
        highprice24h: message_clone.data.highPrice24h,
        lowprice24h: message_clone.data.lowPrice24h,
        prevprice1h: message_clone.data.prevPrice1h,
        markprice: message_clone.data.markPrice,
        indexprice: message_clone.data.indexPrice,
        openinterest: message_clone.data.openInterest,
        openinterestvalue: message_clone.data.openInterestValue,
        turnover24h: message_clone.data.turnover24h,
        volume24h: message_clone.data.volume24h,
        nextfundingtime: message_clone.data.nextFundingTime.map(|dt| dt.naive_utc()),
        fundingrate: message_clone.data.fundingRate,
        bid1price: message_clone.data.bid1Price,
        bid1size: message_clone.data.bid1Size,
        ask1price: message_clone.data.ask1Price,
        ask1size: message_clone.data.ask1Size,
        cs: Some(message_clone.cs),
        ts: Some(message_clone.ts.naive_utc()),
        endpoint: Some(endpoint_name),
        receivedat: received_at_time,
    };

    trace!("New message to insert: {:?}", new_message);

    // Use Diesel's insert_into and execute to insert the new record
    match diesel::insert_into(tickers::table)
        .values(&new_message)
        .execute(conn)
    {
        Ok(_) => trace!("Insertion successful"),
        Err(e) => error!("Error inserting data: {:?}", e),
    }

    trace!("Inserted message: {:?}", message);

    Ok(())
}

pub async fn insert_into_db(mut receiver: broadcast::Receiver<MyMessage>, pool: PgPool) {
    while let Ok(my_msg) = receiver.recv().await {
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

                            // Nu we weten dat het topic met "tickers" begint, zetten we om naar MessageData
                            if let Ok(parsed_message) = from_str::<MessageData>(text) {
                                // Verwerk parsed_message
                                let mut conn = pool.get().expect("Failed to get database connection from pool");
                                let _ = insert_message_into_db(
                                    my_msg.receivedat,
                                    my_msg.endpoint_name,
                                    parsed_message.clone(),
                                    &mut conn,
                                ).await;
                                trace!("Passed to insert: {:?}", parsed_message.clone());
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
}

