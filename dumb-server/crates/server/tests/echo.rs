//! HTTP probe test: `/health` answers 200 `{"status":"ok"}`.
//! (Formerly the T2.1 `/ws` echo test; `/ws` now talks to the Zone, see `zone_e2e.rs`.)

use server::config::Config;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn health_returns_ok_json() {
    let config = Config {
        bind: String::new(),
        tick_hz: 20,
        grace_ms: 5000,
        speed: 5.0,
        world_half: 50.0,
        database_url: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = server::http::router(server::zone::spawn(&config, None));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with(r#"{"status":"ok"}"#), "{response}");
}
