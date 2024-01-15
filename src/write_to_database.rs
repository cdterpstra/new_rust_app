use std::str::FromStr;
use serde_json::{from_str, Value};
use crate::websocket_manager::MyMessage;
use diesel::prelude::*;
use log::info;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::mpsc;
use tungstenite::Message as WebSocketMessage;
use crate::db_connection_manager::PgPool;
use crate::schema::crypto::tickers;
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use diesel::data_types::PgTimestamp;
use diesel::dsl::date;

fn deserialize_optional_string_timestamp<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
{
    let option = Option::<String>::deserialize(deserializer)?;
    match option {
        Some(s) => {
            let millis = i64::from_str(&s).map_err(serde::de::Error::custom)?;
            let timestamp = Utc.timestamp_millis(millis);
            Ok(Some(timestamp))
        },
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
}


pub async fn insert_message_into_db(
    endpoint_name: String,
    message: MessageData,
    conn: &mut PgConnection // Change the reference to mutable
) -> Result<(), diesel::result::Error> {
    let message_clone = message.clone();
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
    };

    println!("New message to insert: {:?}", new_message);

    // Use Diesel's insert_into and execute to insert the new record
    match diesel::insert_into(tickers::table)
        .values(&new_message)
        .execute(conn) {
        Ok(_) => println!("Insertion successful"),
        Err(e) => eprintln!("Error inserting data: {:?}", e),
    }


    println!("Inserted message: {:?}", message);

    Ok(())
}

pub async fn insert_into_db(mut receiver: mpsc::Receiver<MyMessage>, pool: PgPool) {
    while let Some(my_msg) = receiver.recv().await {
        // Handle only text messages
        info!(
            "Received Message with timestamp {} from {}: {:?}",
            my_msg.timestamp, my_msg.endpoint_name, my_msg.message
        );

        if let WebSocketMessage::Text(ref text) = my_msg.message {
            // Now `text` contains your JSON string

            // Deserialize the JSON string to MessageData (defined in previous messages)
            match from_str::<MessageData>(text) {
                Ok(parsed_message) => {
                    // Check if the topic starts with "tickers"
                    if parsed_message.topic.starts_with("tickers") {
                        println!("Received: {:?}", parsed_message.clone());

                        // Obtain a connection from the pool and get a mutable reference
                        let mut conn = pool.get().expect("Failed to get database connection from pool");

                        // Call async function to insert the message with the connection
                        let _ = insert_message_into_db(my_msg.endpoint_name, parsed_message.clone(), &mut conn).await; // Pass &mut conn here
                        println!("Passed to insert: {:?}", parsed_message.clone());
                    }
                },
                Err(e) => {
                    // Handle JSON parsing error
                    eprintln!("Error parsing message JSON: {:?}", e);
                }
            }
        }
    }
}