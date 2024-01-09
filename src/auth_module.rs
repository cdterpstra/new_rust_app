use std::time::SystemTime;
use base64::Engine;
use openssl::{hash::MessageDigest, rsa::Rsa, sign::Signer, pkey::PKey};
use base64::engine::general_purpose::STANDARD;
use config::{Config, File};
use log::debug;
use serde_json::json;
use crate::config::AppConfig;

pub async fn generate_auth_message() -> Result<String, String> {
    // Set up configuration
    let settings = Config::builder()
        .add_source(File::with_name("config/default"))
        .build()
        .map_err(|e| e.to_string())?;

    let app_config: AppConfig = settings.try_deserialize()
        .map_err(|e| e.to_string())?;

    let expires = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
        + 10000;

    let val = format!("GET/realtime{}", expires);

    let rsa = Rsa::private_key_from_pem(app_config.private_key.as_bytes())
        .map_err(|e| e.to_string())?;
    let pkey = PKey::from_rsa(rsa).map_err(|e| e.to_string())?;

    let mut signer = Signer::new(MessageDigest::sha256(), &pkey).map_err(|e| e.to_string())?;
    signer.update(val.as_bytes()).map_err(|e| e.to_string())?;

    let signature = signer.sign_to_vec().map_err(|e| e.to_string())?;
    let base64_config = STANDARD;
    let encoded_signature = base64_config.encode(signature);

    debug!("Generated auth signature: {}", encoded_signature);

    // Construct the JSON message
    let auth_message = json!({
        "op": "auth",
        "args": [app_config.api_key, expires, encoded_signature]
    });

    // Serialize JSON to string
    let message_str = serde_json::to_string(&auth_message)
        .map_err(|e| e.to_string())?;

    debug!("Generated auth message: {}", message_str);

    Ok(message_str)
}