use anyhow::Result;
use tokio::sync::broadcast;
use tracing::warn;

pub type Wav = bytes::Bytes;

pub fn player(mut rx: broadcast::Receiver<Wav>) -> Result<()> {
    let (_stream, handle) = rodio::OutputStream::try_default()?;
    let sink = rodio::Sink::try_new(&handle)?;

    loop {
        let audio_wav = rx.blocking_recv()?;

        // Clear the queue, which pauses it, and being playing again
        // This allows us to focus on playing the most recently received text
        sink.clear();
        sink.play();
        let audio_cursor = std::io::Cursor::new(audio_wav);

        let decoded_audio = match rodio::Decoder::new(std::io::BufReader::new(audio_cursor)) {
            Ok(audio) => audio,
            Err(err)  => { warn!("Failed decoding audio during playback: {}", err); continue; }
        };

        sink.append(decoded_audio);
    }
}
