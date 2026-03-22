use super::*;

#[test]
fn classify_http_path_handles_expected_routes_and_falls_back_for_assets() {
    assert_eq!(classify_http_path("/"), HttpRouteLabel::Root);
    assert_eq!(classify_http_path("/healthz"), HttpRouteLabel::Healthz);
    assert_eq!(classify_http_path("/metrics"), HttpRouteLabel::Metrics);
    assert_eq!(classify_http_path("/adminz"), HttpRouteLabel::Admin);
    assert_eq!(
        classify_http_path("/session/bootstrap"),
        HttpRouteLabel::SessionBootstrap
    );
    assert_eq!(classify_http_path("/ws"), HttpRouteLabel::WebSocket);
    assert_eq!(classify_http_path("/ws-dev"), HttpRouteLabel::WebSocket);
    assert_eq!(classify_http_path("/index.js"), HttpRouteLabel::StaticAsset);
    assert_eq!(
        classify_http_path("/healthz/extra"),
        HttpRouteLabel::StaticAsset
    );
}

#[test]
fn websocket_session_gauge_never_underflows() {
    let observability = ServerObservability::new("test");
    observability.record_websocket_disconnect();

    let metrics = observability.render_prometheus();
    assert!(metrics.contains("rarena_websocket_sessions_active 0"));
    assert!(metrics.contains("rarena_websocket_disconnects_total 1"));
}

#[test]
fn prometheus_render_includes_http_websocket_ingress_and_tick_metrics() {
    let observability = ServerObservability::new("0.8.0-test");
    observability.record_http_request(HttpRouteLabel::Root);
    observability.record_http_request(HttpRouteLabel::Healthz);
    observability.record_http_request(HttpRouteLabel::Metrics);
    observability.record_http_request(HttpRouteLabel::Admin);
    observability.record_websocket_upgrade_attempt();
    observability.record_websocket_session_bound();
    observability.record_ingress_packet(true);
    observability.record_ingress_packet(false);
    observability.record_tick(Duration::from_millis(12));
    observability.record_websocket_disconnect();

    let metrics = observability.render_prometheus();
    assert!(metrics.contains("rarena_http_requests_total{route=\"root\"} 1"));
    assert!(metrics.contains("rarena_http_requests_total{route=\"healthz\"} 1"));
    assert!(metrics.contains("rarena_http_requests_total{route=\"metrics\"} 1"));
    assert!(metrics.contains("rarena_http_requests_total{route=\"admin\"} 1"));
    assert!(metrics.contains("rarena_websocket_upgrade_attempts_total 1"));
    assert!(metrics.contains("rarena_websocket_sessions_bound_total 1"));
    assert!(metrics.contains("rarena_websocket_disconnects_total 1"));
    assert!(metrics.contains("rarena_ingress_packets_total{result=\"accepted\"} 1"));
    assert!(metrics.contains("rarena_ingress_packets_total{result=\"rejected\"} 1"));
    assert!(metrics.contains("rarena_tick_iterations_total 1"));
    assert!(metrics.contains("rarena_build_info{version=\"0.8.0-test\"} 1"));
    assert_eq!(observability.websocket_sessions_bound_total(), 1);
    assert_eq!(observability.websocket_sessions_active(), 0);
    assert_eq!(observability.websocket_disconnects_total(), 1);
    assert_eq!(observability.websocket_rejections_total(), 0);
    assert_eq!(observability.ingress_packets_accepted_total(), 1);
    assert_eq!(observability.ingress_packets_rejected_total(), 1);
    assert_eq!(observability.tick_iterations(), 1);
    assert_eq!(
        observability.tick_duration_last(),
        Duration::from_millis(12)
    );
    assert_eq!(observability.tick_duration_max(), Duration::from_millis(12));
    assert!(observability.uptime() <= Duration::from_secs(1));
}
