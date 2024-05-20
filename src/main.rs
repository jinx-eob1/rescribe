use anyhow::Result;
use reqwest::Url;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

async fn play_wav(audio_wav: bytes::Bytes) -> Result<()> {
    let (_stream, handle) = rodio::OutputStream::try_default()?;
    let sink = rodio::Sink::try_new(&handle)?;
    let audio_cursor = std::io::Cursor::new(audio_wav);
    sink.append(rodio::Decoder::new(std::io::BufReader::new(audio_cursor)).unwrap());
    sink.sleep_until_end();

    Ok(())
}

async fn query_voicevox(input_data: &[u8]) -> Result<bytes::Bytes> {
    let text = std::str::from_utf8(input_data).unwrap().to_string();

    // Url encode
    let url = Url::parse(&std::format!("http://127.0.0.1:50021/audio_query?speaker=1&text={}", text))?.to_string();

    let client = reqwest::Client::new();

    let res = client
        .post(url)
        .send()
        .await?;

    println!("Response status: {}", res.status());
    let json_voice_data = res.text().await?;
    //println!("Response body: {}", json_voice_data);

    let res = client
        .post("http://127.0.0.1:50021/synthesis?speaker=1")
        .body(json_voice_data)
        .send()
        .await?;

    println!("Response status: {}", res.status());
    Ok(res.bytes().await?)
}

struct Message {
    pub _translation: Vec<u8>,
    pub audio_wav: bytes::Bytes
}

async fn tcp_handler(mut socket: TcpStream, msg_queue: Arc<Mutex<VecDeque<Message>>>) {
    let mut buffer = vec![0; 32768];

    // In a loop, read data from the socket and write the data back.
    loop {
        let n = socket
            .read(&mut buffer)
            .await
            .expect("failed to read data from socket");

        // Add valid messages to the queue for processing
        if n > 0 {
            let needle: Vec<u8> = vec![b'\r', b'\n', b'\r', b'\n'];

            let Some(end_pos) = buffer.windows(needle.len()).position(|window| window == needle.as_slice()) else {
                eprintln!("Oversized request or missing \\r\\n\\r\\n");
                continue;
            };

            let buf = &buffer[0..end_pos];
            let audio_wav = query_voicevox(buf).await.unwrap();

            let mut _translation = buffer.clone();
            _translation.resize(end_pos, 0);

            msg_queue.lock().unwrap().push_back(Message {_translation, audio_wav } );
        }
    }
}

async fn queue_reader(msg_queue: Arc<Mutex<VecDeque<Message>>>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        let msg = msg_queue.lock().unwrap().pop_front();

        if let Some(msg) = msg {
            // TODO: err log rather than unwrap
            play_wav(msg.audio_wav).await.unwrap();
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let main_msg_queue: Arc<Mutex<VecDeque<Message>>> = Arc::new(Mutex::new(VecDeque::new()));

    let msg_queue = Arc::clone(&main_msg_queue);
    tokio::spawn(async move {
        queue_reader(msg_queue).await;
    });

    loop {
        let (socket, _) = listener.accept().await?;
        let msg_queue = Arc::clone(&main_msg_queue);
        tokio::spawn(async move {
            tcp_handler(socket, msg_queue).await;
        });
    }
}
