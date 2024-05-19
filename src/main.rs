use anyhow::Result;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use reqwest::Url;

async fn process_socket(socket: TcpStream) -> Result<()> {
    let mut writer = BufWriter::new(socket); 

    let text = "こんにちは、先輩";

    // Url encode
    let url = Url::parse(&std::format!("http://127.0.0.1:50021/audio_query?speaker=1&text={}", text))?.to_string();

    let client = reqwest::Client::new();

    let res = client
        .post(url)
        .send()
        .await?;

    println!("Response status: {}", res.status());
    let json_voice_data = res.text().await?;
    println!("Response body: {}", json_voice_data);

    let res = client
        .post("http://127.0.0.1:50021/synthesis?speaker=1")
        .body(json_voice_data)
        .send()
        .await?;

    println!("Response status: {}", res.status());
    let audio_wav = res.bytes().await?;

    let (_stream, handle) = rodio::OutputStream::try_default()?;
    let sink = rodio::Sink::try_new(&handle)?;
    let audio_cursor = std::io::Cursor::new(audio_wav);
    sink.append(rodio::Decoder::new(std::io::BufReader::new(audio_cursor)).unwrap());
    sink.sleep_until_end();

    writer.write_all(b"success\n").await?;
    writer.flush().await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        process_socket(socket).await?;
    }
}
