use anyhow::Result;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

mod audio;
mod tcp;
mod tts;

struct Message {
    pub _translation: Vec<u8>,
    pub audio_wav: bytes::Bytes
}

async fn queue_reader(msg_queue: Arc<Mutex<VecDeque<Message>>>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        let msg = msg_queue.lock().unwrap().pop_front();

        if let Some(msg) = msg {
            // TODO: err log rather than unwrap
            audio::play_wav(msg.audio_wav).await.unwrap();
        }
    }
}

async fn serve_tcp(main_msg_queue: &Arc<Mutex<VecDeque<Message>>>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let msg_queue = Arc::clone(&main_msg_queue);

        tokio::spawn(async move {
            tcp::handler(socket, msg_queue).await;
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let main_msg_queue: Arc<Mutex<VecDeque<Message>>> = Arc::new(Mutex::new(VecDeque::new()));

    let msg_queue = Arc::clone(&main_msg_queue);
    tokio::spawn(async move {
        queue_reader(msg_queue).await;
    });

    serve_tcp(&main_msg_queue).await?;

    Ok(())
}
