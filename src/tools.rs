use base64::{Engine, prelude::BASE64_STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use tracing::Level;
use tracing_subscriber::fmt::time::LocalTime;

pub fn verify_signature(message: &str, signature: &str, key: &str) -> Result<bool, String> {
    let public_key_bytes = BASE64_STANDARD
        .decode(key)
        .map_err(|e| format!("Invalid public key: {}", e))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes.try_into().unwrap())
        .map_err(|e| format!("Invalid public key: {}", e))?;

    let signature_bytes = BASE64_STANDARD
        .decode(signature)
        .map_err(|e| format!("Invalid signature: {}", e))?;
    let signature = Signature::from_bytes(&signature_bytes.try_into().unwrap());

    Ok(verifying_key.verify(message.as_bytes(), &signature).is_ok())
}

pub fn setup_tracing() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .with_timer(LocalTime::new(time::macros::format_description!(
            "[hour]:[minute]:[second].[subsecond digits:3]"
        )))
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global logger");
}
