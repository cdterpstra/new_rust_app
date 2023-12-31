pub struct ConnectionMessage {
    pub endpoint: String,
    pub connection_timestamp: u128,  // Renamed and type changed to u128
    pub message: String,
}
