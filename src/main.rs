use anyhow::Result;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use futures_util::SinkExt;

mod audio;
mod tcp;
mod tts;

type Queue<T> = Arc<Mutex<VecDeque<T>>>;

struct Message {
    pub _translation: Vec<u8>,
    pub audio_wav: bytes::Bytes
}

async fn play_audio_queue(msg_queue: Queue<Message>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        let msg = msg_queue.lock().unwrap().pop_front();

        if let Some(msg) = msg {
            // TODO: err log rather than unwrap
            audio::play_wav(msg.audio_wav).await.unwrap();
        }
    }
}

async fn serve_tcp(ws_queue: Queue<Message>, audio_queue: Queue<Message>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let ws_q = Arc::clone(&ws_queue);
        let audio_q = Arc::clone(&audio_queue);

        tokio::spawn(async move {
            tcp::handler(socket, ws_q, audio_q).await;
        });
    }
}

async fn serve_websocket(ws_queue: Queue<Message>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9090").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let ws_q = Arc::clone(&ws_queue);

        let mut ws_stream = tokio_websockets::ServerBuilder::new()
            .accept(socket)
            .await?;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                let msg = ws_q.lock().unwrap().pop_front();

                if let Some(msg) = msg {
                    // TODO: err log rather than unwrap
                    let translated_text = String::from_utf8(msg._translation).unwrap();
                    let tokio_msg = tokio_websockets::Message::text(translated_text);
                    ws_stream.send(tokio_msg).await.unwrap();
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let ws_queue: Queue<Message> = Arc::new(Mutex::new(VecDeque::new()));
    let audio_queue: Queue<Message> = Arc::new(Mutex::new(VecDeque::new()));

    let ws_q = Arc::clone(&ws_queue);
    let audio_q = Arc::clone(&audio_queue);
    let tcp_server = tokio::spawn(async move {
        serve_tcp(ws_q, audio_q).await;
    });

    let ws_q = Arc::clone(&ws_queue);
    let ws_server = tokio::spawn(async move {
        serve_websocket(ws_q).await;
    });

    let audio_q = Arc::clone(&audio_queue);
    let audio_reader = tokio::spawn(async move {
        play_audio_queue(audio_q).await;
    });

    tokio::try_join!(tcp_server, ws_server, audio_reader)?;

    Ok(())
}
