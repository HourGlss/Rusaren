use super::*;

fn temp_record_store_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("rusaren-realtime-webrtc-{unique}.tsv"))
}

fn repo_content_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
}

fn temp_web_client_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after the unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rusaren-webrtc-web-root-{prefix}-{unique}"));
    std::fs::create_dir_all(&root).expect("temporary web client root should be created");
    root
}

fn http_authority(base_url: &str) -> String {
    base_url
        .trim_start_matches("ws://")
        .trim_start_matches("wss://")
        .to_string()
}

async fn http_get(base_url: &str, path: &str) -> (u16, String) {
    let authority = http_authority(base_url);
    let mut stream = tokio::net::TcpStream::connect(&authority)
        .await
        .expect("http connection should succeed");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("http request should be written");

    let mut raw_response = Vec::new();
    stream
        .read_to_end(&mut raw_response)
        .await
        .expect("http response should be readable");

    let response =
        String::from_utf8(raw_response).expect("http response should be valid utf8 for tests");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .expect("http response should contain a header/body split");
    let status_line = head.lines().next().expect("http status line should exist");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .expect("http status line should contain a status code")
        .parse::<u16>()
        .expect("http status code should be numeric");

    (status_code, body.to_string())
}

pub(super) async fn bootstrap_signal_url(base_url: &str) -> String {
    let (status_code, body) = http_get(base_url, "/session/bootstrap").await;
    assert_eq!(status_code, 200, "session bootstrap should return HTTP 200");
    let payload = serde_json::from_str::<Value>(&body).expect("bootstrap JSON should decode");
    let token = payload
        .get("token")
        .and_then(Value::as_str)
        .expect("bootstrap JSON should include a token");
    format!("{base_url}/ws?token={token}")
}

pub(super) async fn start_server_fast() -> (game_api::DevServerHandle, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let server = spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval: Duration::from_millis(10),
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: temp_record_store_path(),
            content_root: repo_content_root(),
            web_client_root: temp_web_client_root("fast"),
            observability: None,
            webrtc: WebRtcRuntimeConfig::default(),
        },
    )
    .await
    .expect("server should spawn");
    let base_url = format!("ws://{}", server.local_addr());
    (server, base_url)
}
