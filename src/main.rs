use anyhow::{Result, Context};
use clap::Parser;
use http::QueuePacket;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tracing::{error, info, warn};

mod audio;
mod db;
mod tts;
mod http;

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

async fn generate_tts(mut rx: broadcast::Receiver<http::QueuePacket>, tx: broadcast::Sender<audio::Wav>) -> Result<()> {
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

async fn serve_http(rx: broadcast::Receiver<QueuePacket>, tx: broadcast::Sender<QueuePacket>, port: u32) -> Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    let db = db::Db::open("rescribe.db")?;

    let rx = Arc::new(rx);

    let router = axum::Router::new()
        .nest_service("/", ServeDir::new("ui"))
        .route("/ws",        axum::routing::get (http::handle_websocket))     .with_state(rx)
        .route("/queue",     axum::routing::post(http::handle_queue_post))    .with_state(tx.clone())
        .route("/translate", axum::routing::post(http::handle_translate_post)).with_state((db, tx));

    info!("Listening on port {}", port);
    axum::serve(listener, router).await?;

    Ok(())
}

#[derive(clap::Parser)]
pub struct Args {
    #[arg(short='l', long="log-level", help = "Tracing log level")]
    log_level: Option<LogLevel>,

    #[arg(long, default_value = "7625", help = "Port for language data POST input & websocket output")]
    port: Option<u32>,
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

    // Queue channels for moving data through
    let (queue_tx, queue_rx) = broadcast::channel::<http::QueuePacket>(64);
    let (audio_tx, audio_rx) = broadcast::channel::<audio::Wav>(64);

    let queue_ws_rx  = queue_rx.resubscribe();
    let queue_tts_rx = queue_rx;

    // Handle websocket output and post request input
    let http_server = tokio::spawn(async move {
        return serve_http(queue_ws_rx, queue_tx, args.port.unwrap()).await;
    });

    // Create TTS from POSTed data
    let tts_generator = tokio::spawn(async move {
        return generate_tts(queue_tts_rx, audio_tx).await;
    });

    // Play audio from generated TTS
    // Rodio is not async, so we use blocking here instead
    let audio_player = tokio::task::spawn_blocking(move || {
        audio::player(audio_rx)
    });

    let res = tokio::select! {
        res = http_server   => res.unwrap(),
        res = audio_player  => res.unwrap(),
        res = tts_generator => res.unwrap()
    };

    if let Err(err) = res {
        error!("Error: {}", err);
    }

    Ok(())
}
