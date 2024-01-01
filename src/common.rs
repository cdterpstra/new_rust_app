#[derive(Clone, Debug)]
pub struct ConnectionMessage {
    pub endpoint: Option<String>,
    pub connection_timestamp: u128,
    pub message: String,
}
