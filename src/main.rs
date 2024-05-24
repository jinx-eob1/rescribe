use anyhow::{Result, Context};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::sync::atomic;
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};

mod audio;
mod tcp;
mod tts;

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

#[derive(Clone, Debug)]
struct Message {
    pub translated_text: String,
    pub audio_wav: bytes::Bytes
}

async fn play_audio_queue(mut rx: tokio::sync::broadcast::Receiver<Message>) -> Result<()> {
    loop {
        let msg = rx.recv().await?;
        audio::play_wav(msg.audio_wav).await?;
    }
}

async fn serve_tcp(tx: tokio::sync::broadcast::Sender<Message>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7625").await?;

    loop {
        let (socket, _) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!("Failed to accept tcp connection {}", err);
                continue;
            }
        };
        info!("Obtained new TCP client");
        let tx = tx.clone();

        tokio::spawn(async move {
            return tcp::handler(socket, tx).await;
        });
    }
}

async fn serve_websocket(rx: tokio::sync::broadcast::Receiver<Message>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9090").await?;

    loop {
        let (socket, _) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                warn!("Failed to accept tcp connection {}", err);
                continue;
            }
        };
        let ws_stream = match tokio_websockets::ServerBuilder::new().accept(socket).await {
            Ok(v) => v,
            Err(err) => {
                warn!("Failed to make WS connection {}", err);
                continue;
            }
        };

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
                    let tokio_msg = tokio_websockets::Message::text(msg.translated_text);

                    // Assume socket is closed
                    if let Err(_err) = writer.send(tokio_msg).await {
                        break;
                    }
                }
            }
        });
    }
}

#[derive(clap::Parser)]
pub struct Args {
    #[arg(short='l', long="log-level", help = "Tracing log level")]
    log_level: Option<LogLevel>,
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

    let (msg_tx, msg_rx) = tokio::sync::broadcast::channel::<Message>(64);
    let msg_rx_ws = msg_rx.resubscribe();
    let msg_rx_audio = msg_rx;

    let tcp_server: tokio::task::JoinHandle<Result<(), anyhow::Error>> = tokio::spawn(async move {
        return serve_tcp(msg_tx).await;
    });

    let ws_server = tokio::spawn(async move {
        return serve_websocket(msg_rx_ws).await;
    });

    let audio_reader = tokio::spawn(async move {
        return play_audio_queue(msg_rx_audio).await;
    });

    let res = tokio::select! {
        res = tcp_server   => res.unwrap(),
        res = ws_server    => res.unwrap(),
        res = audio_reader => res.unwrap()
    };

    if let Err(err) = res {
        error!("Error: {}", err);
    }

    Ok(())
}
