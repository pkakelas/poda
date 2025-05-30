use base64::{Engine};

pub fn base64_engine() -> impl Engine {
    base64::engine::general_purpose::STANDARD
}
