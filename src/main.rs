use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_websockets::WebSocketStream;
use tokio::signal::unix::{signal, SignalKind};

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
        let msg = msg_queue.lock().unwrap().pop_front();

        if let Some(msg) = msg {
            // TODO: err log rather than unwrap
            audio::play_wav(msg.audio_wav).await.unwrap();
        }
        else {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
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

async fn serve_websocket(msg_queue: Queue<Message>) -> Result<()> {
    type SocketVec = Arc<tokio::sync::Mutex<Vec<WebSocketStream<TcpStream>>>>;
    let listener = TcpListener::bind("127.0.0.1:9090").await?;
    let ws_streams: SocketVec = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let ws_streams_accept = Arc::clone(&ws_streams);
    let _conn_accepter = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let ws_stream = tokio_websockets::ServerBuilder::new()
                .accept(socket)
                .await.unwrap();

            ws_streams_accept.lock().await.push(ws_stream);
        }
    });

    loop {

        if ws_streams.lock().await.is_empty() || msg_queue.lock().unwrap().is_empty() {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            continue;
        }

        let msg = msg_queue.lock().unwrap().pop_front();

        if let Some(msg) = msg {
            let translated_text = String::from_utf8(msg._translation).unwrap();
            let tokio_msg = tokio_websockets::Message::text(translated_text);

            let mut ws_stream_locked = ws_streams.lock().await;

            let mut i = 0;

            while i < ws_stream_locked.len() {
                let stream = ws_stream_locked.get_mut(i).unwrap();

                if let Err(_err) = stream.send(tokio_msg.clone()).await {
                    ws_stream_locked.swap_remove(i);
                }
                else {
                    i += 1;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Ignore sigpipe
    let mut _sigpipe = signal(SignalKind::pipe()).unwrap();

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
