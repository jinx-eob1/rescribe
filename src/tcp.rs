use anyhow::Result;
use tokio::io::AsyncBufReadExt;
use tokio::net::TcpStream;
use tracing::{info, warn};

use crate::tts;
use crate::Message;

async fn read_until_end_sequence(reader: &mut tokio::io::BufReader<TcpStream>) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::new();

    loop {
        let read_until = reader.read_until(b'\n', &mut buffer);

        match read_until.await {
            Ok(bytes_read) => {
                if bytes_read == 0 { break; }
            },
            Err(err) => {
                warn!("voicevox tts error: {}", err);
                break;
            }
        }

        if buffer.ends_with(b"\r\n\r\n") {
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

        let audio_wav = match tts::voicevox(&buffer).await {
            Ok(wav) => wav,
            Err(err) => {
                warn!("Error with voicevox tts {}", err);
                continue;
            }
        };

        let translated_text = String::from_utf8(buffer.clone()).unwrap();
        info!("Received: {}", translated_text);
        let msg = Message{ _translation: buffer, audio_wav };

        tx.send(msg)?;
    }

    Ok(())
}
