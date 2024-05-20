use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncBufReadExt;
use tokio::net::TcpStream;

use crate::tts;
use crate::Message;

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

pub async fn handler(socket: TcpStream, msg_queue: Arc<Mutex<VecDeque<Message>>>) {
    let mut reader = tokio::io::BufReader::new(socket);

    loop {
        let buffer = read_until_end_sequence(&mut reader).await;

        if !buffer.is_empty() {
            let audio_wav = tts::voicevox(&buffer).await.unwrap();

            msg_queue.lock().unwrap().push_back(Message {_translation: buffer, audio_wav } );
        }
    }
}
