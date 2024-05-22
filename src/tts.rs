use anyhow::Result;
use reqwest::Url;

pub async fn voicevox(input_data: &[u8]) -> Result<bytes::Bytes> {
    let text = std::str::from_utf8(input_data).unwrap().to_string();

    // Url encode
    let url = Url::parse(&std::format!("http://127.0.0.1:50021/audio_query?speaker=1&text={}", text))?.to_string();

    let client = reqwest::Client::new();

    let res = client
        .post(url)
        .send()
        .await?;

    let json_voice_data = res.text().await?;

    let res = client
        .post("http://127.0.0.1:50021/synthesis?speaker=1")
        .body(json_voice_data)
        .send()
        .await?;

    Ok(res.bytes().await?)
}

