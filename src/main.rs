use anyhow::Result;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use reqwest::Url;

async fn process_data(input_data: &[u8]) -> Result<()> {
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

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = vec![0; 32768];

            // In a loop, read data from the socket and write the data back.
            loop {
                let n = socket
                    .read(&mut buf)
                    .await
                    .expect("failed to read data from socket");

                if n == 0 {
                    return;
                }

                let needle: Vec<u8> = vec![b'\r', b'\n', b'\r', b'\n'];

                let Some(pos) = buf.windows(needle.len()).position(|window| window == needle.as_slice()) else {
                    eprintln!("Oversized request or missing \\r\\n\\r\\n");
                    continue;
                };

                let input_slice = &buf[0..pos];

                process_data(input_slice).await.unwrap();
            }
        });
    }
}
