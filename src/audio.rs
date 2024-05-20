use anyhow::Result;

pub async fn play_wav(audio_wav: bytes::Bytes) -> Result<()> {
    let (_stream, handle) = rodio::OutputStream::try_default()?;
    let sink = rodio::Sink::try_new(&handle)?;
    let audio_cursor = std::io::Cursor::new(audio_wav);
    sink.append(rodio::Decoder::new(std::io::BufReader::new(audio_cursor)).unwrap());
    sink.sleep_until_end();

    Ok(())
}
