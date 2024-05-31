use serde::Deserialize;
use tracing::info;
use tokio::sync::broadcast;
use axum::extract::State;

#[derive(Clone, Deserialize, Debug)]
pub struct QueuePacket {
    pub language: String,
    pub original_text: Option<String>,
    pub translated_text: String
}

pub async fn handler(State(tx): State<broadcast::Sender<QueuePacket>>, axum::Json(packet): axum::Json<QueuePacket>) {
    info!("Received translation: {:?}", packet);

    tx.send(packet).unwrap();
}
