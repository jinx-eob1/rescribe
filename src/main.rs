use anyhow::Result;
use reqwest::Url;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncBufReadExt;
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

async fn read_until_end_sequence(reader: &mut tokio::io::BufReader<TcpStream>) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut bytes_read;

    loop {
        let read_until = reader.read_until(b'\n', &mut buffer);
        bytes_read = read_until.await.unwrap();
        if bytes_read == 0 {
            break;
        }

        if buffer.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    buffer
}

async fn tcp_handler(socket: TcpStream, msg_queue: Arc<Mutex<VecDeque<Message>>>) {
    let mut reader = tokio::io::BufReader::new(socket);

    loop {
        let buffer = read_until_end_sequence(&mut reader).await;

        if !buffer.is_empty() {
            let audio_wav = query_voicevox(&buffer).await.unwrap();

            msg_queue.lock().unwrap().push_back(Message {_translation: buffer, audio_wav } );
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
