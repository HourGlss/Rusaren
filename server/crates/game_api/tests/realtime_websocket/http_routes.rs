use super::*;
use base64::Engine as _;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_root_serves_the_exported_web_shell_and_keeps_websocket_routes_alive() {
    let web_client_root = temp_web_client_root(
        "hosted-shell",
        Some(
            "<!doctype html><html><head><title>Rusaren Control Shell</title></head><body><script src=\"index.js\"></script></body></html>",
        ),
    );
    let (server, base_url) = start_server_with_web_root(web_client_root).await;

    let (status_code, index_body) = http_get(&base_url, "/").await;
    assert_eq!(status_code, 200);
    assert!(index_body.contains("Rusaren Control Shell"));

    let (asset_status_code, asset_body) = http_get(&base_url, "/index.js").await;
    assert_eq!(asset_status_code, 200);
    assert!(asset_body.contains("rusaren shell"));

    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    connect_player(&mut socket, "Alice").await;
    let connect_events = recv_events_until(&mut socket, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    assert!(connect_events
        .iter()
        .any(|event| matches!(event, ServerControlEvent::Connected { .. })));

    let _ = socket.close(None).await;
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hosted_root_returns_a_clear_placeholder_when_the_web_bundle_is_missing() {
    let web_client_root = temp_web_client_root("missing-shell", None);
    let (server, base_url) = start_server_with_web_root(web_client_root).await;

    let (status_code, body) = http_get(&base_url, "/").await;
    assert_eq!(status_code, 503);
    assert!(body.contains("Rusaren web client is not built yet."));
    assert!(body.contains("export-web-client.ps1"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn healthcheck_and_metrics_routes_report_expected_status_and_prometheus_text() {
    let observability = ServerObservability::new("test-metrics");
    let web_client_root = temp_web_client_root(
        "metrics-shell",
        Some("<!doctype html><html><body>metrics shell</body></html>"),
    );
    let (server, base_url) = start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_millis(10),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root,
        observability: Some(observability.clone()),
        webrtc: WebRtcRuntimeConfig::default(),
        admin_auth: None,
    })
    .await;

    let (health_status, health_body) = http_get(&base_url, "/healthz").await;
    assert_eq!(health_status, 200);
    assert_eq!(health_body, "ok");

    let (root_status, root_body) = http_get(&base_url, "/").await;
    assert_eq!(root_status, 200);
    assert!(root_body.contains("metrics shell"));

    let mut socket = connect_socket(&bootstrap_signal_url(&base_url).await).await;
    connect_player(&mut socket, "Alice").await;
    let _ = recv_events_until(&mut socket, 3, |event| {
        matches!(event, ServerControlEvent::LobbyDirectorySnapshot { .. })
    })
    .await;
    let _ = socket.close(None).await;

    tokio::time::sleep(Duration::from_millis(20)).await;

    let (metrics_status, metrics_body) = http_get(&base_url, "/metrics").await;
    assert_eq!(metrics_status, 200);
    assert!(metrics_body.contains("rarena_http_requests_total{route=\"healthz\"} 1"));
    assert!(metrics_body.contains("rarena_http_requests_total{route=\"root\"} 1"));
    assert!(metrics_body.contains("rarena_http_requests_total{route=\"metrics\"} 1"));
    assert!(metrics_body.contains("rarena_websocket_upgrade_attempts_total 1"));
    assert!(metrics_body.contains("rarena_websocket_sessions_bound_total 1"));
    assert!(metrics_body.contains("rarena_websocket_disconnects_total 1"));
    assert!(metrics_body.contains("rarena_build_info{version=\"test-metrics\"} 1"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metrics_route_returns_service_unavailable_when_observability_is_disabled() {
    let web_client_root = temp_web_client_root(
        "metrics-disabled",
        Some("<!doctype html><html><body>metrics disabled shell</body></html>"),
    );
    let (server, base_url) = start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_secs(1),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root,
        observability: None,
        webrtc: WebRtcRuntimeConfig::default(),
        admin_auth: None,
    })
    .await;

    let (status_code, body) = http_get(&base_url, "/metrics").await;
    assert_eq!(status_code, 503);
    assert!(body.contains("Rusaren metrics are disabled"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_dashboard_requires_basic_auth_and_renders_runtime_state() {
    let observability = ServerObservability::new("test-admin");
    let web_client_root = temp_web_client_root(
        "admin-dashboard",
        Some("<!doctype html><html><body>admin shell</body></html>"),
    );
    let (server, base_url) = start_server_with_options(DevServerOptions {
        tick_interval: Duration::from_millis(10),
        simulation_step_ms: COMBAT_FRAME_MS,
        record_store_path: temp_record_store_path(),
        content_root: repo_content_root(),
        web_client_root,
        observability: Some(observability),
        webrtc: WebRtcRuntimeConfig::default(),
        admin_auth: Some(
            game_api::AdminAuthConfig::new("admin", "secret-password")
                .expect("admin auth should parse"),
        ),
    })
    .await;

    let (unauthorized_status, unauthorized_body) = http_get(&base_url, "/adminz").await;
    assert_eq!(unauthorized_status, 401);
    assert!(unauthorized_body.contains("Rusaren admin authentication required"));

    let auth_header = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("admin:secret-password")
    );
    let (authorized_status, authorized_body) =
        http_get_with_headers(&base_url, "/adminz", &[("Authorization", &auth_header)]).await;
    assert_eq!(authorized_status, 200);
    assert!(authorized_body.contains("Rusaren Admin Dashboard"));
    assert!(authorized_body.contains("Connected players"));
    assert!(authorized_body.contains("Prometheus Snapshot"));

    server.shutdown().await;
}
