use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use std::sync::Arc;
use std::sync::atomic;

mod audio;
mod tcp;
mod tts;

#[derive(Clone, Debug)]
struct Message {
    pub _translation: Vec<u8>,
    pub audio_wav: bytes::Bytes
}

async fn play_audio_queue(mut rx: tokio::sync::broadcast::Receiver<Message>) {
    loop {
        let msg = rx.recv().await;

        // TODO: err log rather than unwrap
        if let Ok(msg) = msg {
            audio::play_wav(msg.audio_wav).await.unwrap();
        }
    }
}

async fn serve_tcp(tx: tokio::sync::broadcast::Sender<Message>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let tx = tx.clone();

        tokio::spawn(async move {
            tcp::handler(socket, tx).await;
        });
    }
}

async fn serve_websocket(rx: tokio::sync::broadcast::Receiver<Message>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9090").await?;

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let ws_stream = tokio_websockets::ServerBuilder::new()
            .accept(socket)
            .await.unwrap();

        let (mut writer, mut reader) = ws_stream.split();
        let alive_reader = Arc::new(atomic::AtomicBool::new(true));
        let alive_write = Arc::clone(&alive_reader);

        tokio::spawn(async move {
            while let Some(Ok(_)) = reader.next().await { }
            alive_reader.store(false, atomic::Ordering::Relaxed);
        });

        let mut rx = rx.resubscribe();
        tokio::spawn(async move {
            loop {
                let msg = rx.recv().await;

                let alive = alive_write.load(atomic::Ordering::Relaxed);
                if !alive {
                    break;
                }
        
                if let Ok(msg) = msg {
                    let translated_text = String::from_utf8(msg._translation).unwrap();
                    let tokio_msg = tokio_websockets::Message::text(translated_text);
                    if let Err(_err) = writer.send(tokio_msg).await {
                        break;
                    }
                }
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Ignore sigpipe
    let mut _sigpipe = signal(SignalKind::pipe()).unwrap();

    let (msg_tx, mut msg_rx) = tokio::sync::broadcast::channel::<Message>(64);
    let msg_rx_ws = msg_rx.resubscribe();
    let msg_rx_audio = msg_rx;

    let tcp_server = tokio::spawn(async move {
        serve_tcp(msg_tx).await;
    });

    let ws_server = tokio::spawn(async move {
        serve_websocket(msg_rx_ws).await;
    });

    let audio_reader = tokio::spawn(async move {
        play_audio_queue(msg_rx_audio).await;
    });

    tokio::try_join!(tcp_server, ws_server, audio_reader)?;

    Ok(())
}
