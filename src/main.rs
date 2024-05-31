use anyhow::{Result, Context};
use axum::extract::State;
use axum::extract::ws::{WebSocketUpgrade, WebSocket};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use http::QueuePacket;
use std::sync::Arc;
use std::sync::atomic;
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, warn};
use tokio::sync::broadcast;

mod audio;
mod tts;
mod http;

type AudioWav = bytes::Bytes;

#[derive(Copy, Clone, clap::ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error
}

impl LogLevel {
    pub fn to_trace(&self) -> tracing::Level {
        match self {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info  => tracing::Level::INFO,
            LogLevel::Warn  => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

async fn serve_audio(mut rx: broadcast::Receiver<AudioWav>) -> Result<()> {
    loop {
        let audio = rx.recv().await?;

        if let Err(err) = tokio::task::spawn_blocking(move || {
            audio::play_wav(audio)
        }).await {
            warn!("Failed playing audio: {}", err);
        }
    }
}

async fn serve_tts(mut rx: broadcast::Receiver<http::QueuePacket>, tx: broadcast::Sender<AudioWav>) -> Result<()> {
    loop {
        let msg = rx.recv().await?;

        let audio_wav = match tts::process(&msg.language, &msg.translated_text).await {
            Ok(wav) => wav,
            Err(err) => {
                warn!("TTS err: {}", err);
                continue;
            }
        };

        tx.send(audio_wav)?;
    }
}

async fn handle_websocket(socket: WebSocket, rx: Arc<broadcast::Receiver<QueuePacket>>) {
    let (mut writer, mut reader) = socket.split();
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
                let tokio_msg = axum::extract::ws::Message::Text(msg.translated_text);

                // Assume socket is closed
                if let Err(_err) = writer.send(tokio_msg).await {
                    break;
                }
            }
        }
    });
}

async fn ws_handler(ws: WebSocketUpgrade, State(rx): State<Arc<broadcast::Receiver<QueuePacket>>>) -> axum::response::Response {
    ws.on_upgrade(|socket| handle_websocket(socket, rx))
}

async fn serve_http(rx: broadcast::Receiver<QueuePacket>, tx: broadcast::Sender<QueuePacket>, port: u32) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    let rx = Arc::new(rx);

    let router = axum::Router::new()
        .route("/ws", axum::routing::get(ws_handler))
            .with_state(rx)
        .route("/queue", axum::routing::post(http::handler))
            .with_state(tx.clone());

    axum::serve(listener, router).await?;

    Ok(())
}

#[derive(clap::Parser)]
pub struct Args {
    #[arg(short='l', long="log-level", help = "Tracing log level")]
    log_level: Option<LogLevel>,

    #[arg(long, default_value = "7625", help = "HTTP port for language input data")]
    http_port: Option<u32>,

    #[arg(long, default_value = "7626", help = "Websocket port for ui output")]
    ws_port: Option<u32>,
}

#[tokio::main]
async fn main() ->  Result<()> {
    // Ignore sigpipe
    let mut _sigpipe = signal(SignalKind::pipe())?;

    let args = Args::parse();

    let log_level = args.log_level.unwrap_or(LogLevel::Info).to_trace();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(log_level).finish();

    tracing::subscriber::set_global_default(subscriber).context("setting tracing default failed")?;

    let span = tracing::trace_span!("rescribe");
    let _guard = span.enter();

    let (queue_tx, queue_rx) = broadcast::channel::<http::QueuePacket>(64);
    let (audio_tx, audio_rx) = broadcast::channel::<AudioWav>(64);

    let queue_ws_rx  = queue_rx.resubscribe();
    let queue_tts_rx = queue_rx;

    let http_server = tokio::spawn(async move {
        return serve_http(queue_ws_rx, queue_tx, args.http_port.unwrap()).await;
    });

    let tts_generator = tokio::spawn(async move {
        return serve_tts(queue_tts_rx, audio_tx).await;
    });

    let audio_reader = tokio::spawn(async move {
        return serve_audio(audio_rx).await;
    });

    let res = tokio::select! {
        res = http_server   => res.unwrap(),
        res = audio_reader  => res.unwrap(),
        res = tts_generator => res.unwrap()
    };

    if let Err(err) = res {
        error!("Error: {}", err);
    }

    Ok(())
}
