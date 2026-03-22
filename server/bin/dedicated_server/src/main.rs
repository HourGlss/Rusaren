//! Dedicated server entrypoint.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

mod config;
mod demo;
mod logging;
mod render;

#[cfg(test)]
mod tests;

use std::env;

use game_api::{spawn_dev_server_with_options, DevServerOptions};
use game_sim::COMBAT_FRAME_MS;
use tracing::{error, info};

use crate::config::ServerConfig;
use crate::demo::run_demo;
use crate::logging::{init_tracing, parse_log_format_from_env};

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(error) => {
                error!(%error, "failed to listen for SIGTERM, falling back to ctrl_c only");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };

        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    error!(%error, "failed to listen for ctrl_c");
                }
            }
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        if let Err(error) = tokio::signal::ctrl_c().await {
            error!(%error, "failed to listen for ctrl_c");
        }
    }
}

#[tokio::main]
async fn main() {
    if matches!(env::args().nth(1).as_deref(), Some("--demo")) {
        match run_demo() {
            Ok(lines) => {
                for line in lines {
                    println!("{line}");
                }
            }
            Err(error) => {
                eprintln!("dedicated_server demo failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    let log_format = match parse_log_format_from_env() {
        Ok(log_format) => log_format,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = init_tracing(log_format) {
        eprintln!("{error}");
        std::process::exit(1);
    }

    let config = match ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            error!(%error, "dedicated_server failed to parse configuration");
            std::process::exit(1);
        }
    };

    let listener = match tokio::net::TcpListener::bind(&config.bind_address).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(bind_address = %config.bind_address, %error, "dedicated_server failed to bind");
            std::process::exit(1);
        }
    };

    let server = match spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval: config.tick_interval,
            simulation_step_ms: COMBAT_FRAME_MS,
            record_store_path: config.record_store_path,
            content_root: config.content_root,
            web_client_root: config.web_client_root,
            observability: DevServerOptions::default().observability,
            webrtc: config.webrtc,
            admin_auth: config.admin_auth,
        },
    )
    .await
    {
        Ok(server) => server,
        Err(error) => {
            error!(%error, "dedicated_server failed to start websocket adapter");
            std::process::exit(1);
        }
    };

    info!(
        http_url = %format!("http://{}", server.local_addr()),
        signaling_url = %format!("ws://{}/ws", server.local_addr()),
        websocket_dev_url = %format!("ws://{}/ws-dev", server.local_addr()),
        "dedicated_server listening"
    );
    wait_for_shutdown_signal().await;
    info!("shutdown signal received, stopping dedicated_server");
    server.shutdown().await;
    info!("dedicated_server stopped");
}
