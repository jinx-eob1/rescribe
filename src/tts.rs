use anyhow::{anyhow, Result, Context};
use reqwest::Url;
use tokio::{process::Command, io::AsyncWriteExt};

pub async fn process(language: &str, text: &str) -> Result<bytes::Bytes> {
    match language {
        "es" => piper(text).await,
        "ja" => voicevox(text).await,
        _    => voicevox(text).await
    }
}

async fn piper(text: &str) -> Result<bytes::Bytes> {
    let span = tracing::trace_span!("piper");
    let _guard = span.enter();
    let mut cmd = Command::new("/opt/piper/piper")
        .args([
            "--model", "/opt/piper/es_ES-davefx-medium.onnx",
            "--output_file", "-" // Write wav to stdout
         ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().context("Failed starting piper")?;

    let stdin = cmd.stdin.take().expect("Failed to obtain stdin");
    let mut stdin_writer = tokio::io::BufWriter::new(stdin);

    // Write everything out, flush it, and drop to signify to the process we're done writing to it
    stdin_writer.write_all(text.as_bytes()).await.context("Failed to write to stdin")?;
    stdin_writer.flush().await?;
    drop(stdin_writer);

    let output = cmd.wait_with_output().await.context("Failed waiting for executing to finish")?;

    if output.stdout.len() == 0 {
        return Err(anyhow!("Returned 0 length audio"));
    }

    match output.status.code() {
        Some(code) => {
            if code != 0 {
                let stderr_str: &str = std::str::from_utf8(&output.stderr).unwrap_or("");

                return Err(anyhow!("Failure status code: {} - {}", code, stderr_str));
            }

            let bytes: bytes::Bytes = output.stdout.into();
            Ok(bytes)
        },
        None => Err(anyhow!("Terminated by signal"))
    }
}

async fn voicevox(text: &str) -> Result<bytes::Bytes> {
    let span = tracing::trace_span!("voicevox");
    let _guard = span.enter();
    // Url encode
    let url = Url::parse(&std::format!("http://127.0.0.1:50021/audio_query?speaker=1&text={}", text))?.to_string();

    let client = reqwest::Client::new();

    let res = client
        .post(url)
        .send()
        .await.context("Failed POST")?;

    let json_voice_data = res.text().await?;

    let res = client
        .post("http://127.0.0.1:50021/synthesis?speaker=1")
        .body(json_voice_data)
        .send()
        .await.context("Failed wav GET")?;

    Ok(res.bytes().await.context("Failed to get bytes from GET")?)
}

