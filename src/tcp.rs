use anyhow::Result;
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;
use tokio::net::TcpStream;
use tracing::{info, warn};

use crate::tts;
use crate::Message;

#[derive(Deserialize, Debug)]
struct Packet {
    language: String,
    original_text: Option<String>,
    translated_text: String
}

async fn read_until_end_sequence(reader: &mut tokio::io::BufReader<TcpStream>) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::new();

    loop {
        let read_until = reader.read_until(b'\n', &mut buffer);

        match read_until.await {
            Ok(bytes_read) => {
                if bytes_read == 0 { break; }
            },
            Err(_) => {
                break;
            }
        }

        // Found the end terminator, remove it and return
        // We *should* be returning clean json text from here
        if buffer.ends_with(b"\r\n\r\n") {
            buffer.truncate(buffer.len()-4);
            break;
        }
    }

    buffer
}

pub async fn handler(socket: TcpStream, tx: tokio::sync::broadcast::Sender<Message>) -> Result<()> {
    let mut reader = tokio::io::BufReader::new(socket);

    loop {
        let buffer = read_until_end_sequence(&mut reader).await;

        // Socket closed
        if buffer.is_empty() {
            break;
        }

        let packet: Packet = match serde_json::from_slice(&buffer) {
            Ok(v) => v,
            Err(err) => {
                warn!("Malformed TCP packet received: {}", err);
                continue;
            }
        };

        let audio_wav = match tts::voicevox(&packet.translated_text).await {
            Ok(wav) => wav,
            Err(err) => {
                warn!("Error with voicevox tts {}", err);
                continue;
            }
        };

        info!("Received translation: {}", packet.translated_text);
        let msg = Message{ translated_text: packet.translated_text, audio_wav };

        tx.send(msg)?;
    }

    Ok(())
}
