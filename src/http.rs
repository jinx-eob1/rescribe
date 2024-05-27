use serde::Deserialize;
use tracing::{info, warn};

use crate::tts;
use crate::Message;

#[derive(Deserialize, Debug)]
pub struct Packet {
    language: String,
    original_text: Option<String>,
    translated_text: String
}

pub async fn handler(axum::extract::State(tx): axum::extract::State<tokio::sync::broadcast::Sender<Message>>, axum::Json(packet): axum::Json<Packet>) {
    let audio_wav = match tts::process(&packet.language, &packet.translated_text).await {
        Ok(wav) => wav,
        Err(err) => {
            warn!("TTS err: {}", err);
            return;
        }
    };

    info!("Received translation: {:?}", packet);
    let msg = Message{ translated_text: packet.translated_text, audio_wav };

    tx.send(msg).unwrap();
}
