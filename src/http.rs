use anyhow::{Result, Context};
use axum::extract::State;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::{Arc, atomic};
use tokio::sync::broadcast;
use tracing::{info, error};

#[derive(Clone, Deserialize, Debug)]
pub struct QueuePacket {
    pub language: String,
    pub original_text: Option<String>,
    pub translated_text: String,
    // If we receive a message from the websocket client
    // itself, we do not want to loop by sending it again
    pub prevent_ws_forward: Option<bool>
}

#[derive(Clone, Deserialize, Debug)]
pub struct TranslatePacket {
    pub source_lang: String,
    pub target_lang: String,
    pub text: Vec<String>
}

#[derive(Debug, serde::Deserialize)]
struct DeeplResponseTranslation {
    text: String
}

#[derive(Debug, serde::Deserialize)]
struct DeeplResponse {
    translations: Vec<DeeplResponseTranslation>,
}

// Responds with deepl json response
pub async fn deepl_translate(tx: broadcast::Sender<QueuePacket>, packet: TranslatePacket) -> Result<String> {
    let _span = tracing::trace_span!("translation");
    info!("Received request for translation: {:?}", packet);

    let key = std::env::var("RESCRIBE_DEEPL_KEY").context("RESCRIBE_DEEPL_KEY env var not set")?;

    let endpoint = if key.ends_with(":fx") { "https://api-free.deepl.com/v2/translate" }
                   else                    { "https://api.deepl.com/v2/translate" };


    let body = serde_json::json!({
        "target_lang": packet.target_lang,         
        "split_sentences": "nonewlines",
        "preserve_formatting": true,
        //"formality": "prefer_less",
        //"tag_handling": "xml",
        "source_lang": packet.source_lang,
        "text": packet.text
    }).to_string();

    let client = reqwest::Client::new();
    let res = client
        .post(endpoint)
        .header("User-Agent", "rescribe")
        .header("Authorization", format!("DeepL-Auth-Key {}",  key))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Content-Length", body.len())
        .body(body)
        .send()
        .await;

    let res = res.context("Failed to query deepl")?;
    let res_text = res.text().await.context("Failed getting text from deepl")?;
    let resp: DeeplResponse = serde_json::from_str(&res_text)?;

    for (i, translation) in resp.translations.iter().enumerate() {
        let original_text = packet.text.get(i).map(String::clone);

        let queue_packet = QueuePacket {
            language: packet.target_lang.clone(),
            original_text,
            translated_text: translation.text.clone(),
            prevent_ws_forward: Some(false)
        };

        tx.send(queue_packet).unwrap();
    }

    Ok(res_text)
}

pub async fn handle_translate_post(State(tx): State<broadcast::Sender<QueuePacket>>, axum::Json(packet): axum::Json<TranslatePacket>) -> axum::response::Response {
    match deepl_translate(tx, packet).await {
        //Ok(()) => axum::http::StatusCode::OK.into_response(),
        Ok(res) => (axum::http::StatusCode::OK, res).into_response(),
        Err(e) => {
            error!("Error during translation: {e}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn handle_queue_post(State(tx): State<broadcast::Sender<QueuePacket>>, axum::Json(packet): axum::Json<QueuePacket>) {
    info!("Received translation: {:?}", packet);

    tx.send(packet).unwrap();
}

pub async fn handle_websocket(ws: WebSocketUpgrade, State(rx): State<Arc<broadcast::Receiver<QueuePacket>>>) -> axum::response::Response {
    ws.on_upgrade(|socket| handle_upgraded_websocket(socket, rx))
}

async fn handle_upgraded_websocket(socket: WebSocket, rx: Arc<broadcast::Receiver<QueuePacket>>) {
    let (mut writer, mut reader) = socket.split();
    let alive_reader = Arc::new(atomic::AtomicBool::new(true));
    let alive_write = Arc::clone(&alive_reader);

    // Do nothing on read but mark the connection as dead when reading is finished
    // This prevents us from continuing to write to a disconnected client
    tokio::spawn(async move {
        while let Some(Ok(_)) = reader.next().await { }
        alive_reader.store(false, atomic::Ordering::Relaxed);
    });

    let mut rx = rx.resubscribe();
    tokio::spawn(async move {
        loop {
            let msg = rx.recv().await;

            // Don't try to send data to a dead client
            let alive = alive_write.load(atomic::Ordering::Relaxed);
            if !alive {
                break;
            }
    
            if let Ok(msg) = msg {
                if let Some(prevent_forward) = msg.prevent_ws_forward {
                    if prevent_forward {
                        continue;
                    }
                }

                let tokio_msg = axum::extract::ws::Message::Text(msg.translated_text);

                // Assume socket is closed on error
                if let Err(_err) = writer.send(tokio_msg).await {
                    break;
                }
            }
        }
    });
}
