use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};

async fn process_socket(socket: TcpStream) {
    let mut writer = BufWriter::new(socket); 
    writer.write_all(b"hello\n").await.unwrap();
    writer.flush().await.unwrap();
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        process_socket(socket).await;
    }
}
