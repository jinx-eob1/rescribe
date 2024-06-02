use axum::extract::State;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::{Arc, atomic};
use tokio::sync::broadcast;
use tracing::info;

#[derive(Clone, Deserialize, Debug)]
pub struct QueuePacket {
    pub language: String,
    pub original_text: Option<String>,
    pub translated_text: String,
    // If we receive a message from the websocket client
    // itself, we do not want to loop by sending it again
    pub prevent_ws_forward: Option<bool>
}

pub async fn handle_post(State(tx): State<broadcast::Sender<QueuePacket>>, axum::Json(packet): axum::Json<QueuePacket>) {
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
