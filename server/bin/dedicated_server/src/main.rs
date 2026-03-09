//! Dedicated server entrypoint.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use game_api::{spawn_dev_server_with_options, DevServerOptions};
use game_domain::{
    LobbyId, MatchId, PlayerId, PlayerName, PlayerRecord, ReadyState, SkillChoice, SkillTree,
    TeamSide,
};
use game_lobby::{Lobby, LobbyEvent};
use game_match::{MatchConfig, MatchEvent, MatchSession};
use game_sim::{MovementIntent, SimPlayerSeed, SimulationEvent, SimulationWorld};
use tracing::{error, info};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogFormat {
    Pretty,
    Json,
}

impl LogFormat {
    fn parse(raw: &str) -> Result<Self, String> {
        if raw.eq_ignore_ascii_case("pretty") {
            return Ok(Self::Pretty);
        }
        if raw.eq_ignore_ascii_case("json") {
            return Ok(Self::Json);
        }

        Err(format!(
            "unsupported RARENA_LOG_FORMAT '{raw}'; expected 'pretty' or 'json'"
        ))
    }
}

fn player_id(raw: u32) -> PlayerId {
    match PlayerId::new(raw) {
        Ok(player_id) => player_id,
        Err(error) => panic!("demo player id should be valid: {error}"),
    }
}

fn player_name(raw: &str) -> PlayerName {
    match PlayerName::new(raw) {
        Ok(player_name) => player_name,
        Err(error) => panic!("demo player name should be valid: {error}"),
    }
}

fn skill(tree: SkillTree, tier: u8) -> SkillChoice {
    match SkillChoice::new(tree, tier) {
        Ok(choice) => choice,
        Err(error) => panic!("demo skill choice should be valid: {error}"),
    }
}

fn render_lobby_event(event: &LobbyEvent) -> String {
    match event {
        LobbyEvent::PlayerJoined { player_id } => {
            format!("player {} joined the lobby", player_id.get())
        }
        LobbyEvent::PlayerLeft { player_id } => {
            format!("player {} left the lobby", player_id.get())
        }
        LobbyEvent::TeamSelected {
            player_id,
            team,
            ready_reset,
        } => format!(
            "player {} joined {} (ready reset: {})",
            player_id.get(),
            team,
            ready_reset
        ),
        LobbyEvent::ReadyChanged { player_id, ready } => {
            format!("player {} ready state is now {:?}", player_id.get(), ready)
        }
        LobbyEvent::LaunchCountdownStarted {
            seconds_remaining,
            roster,
        } => format!(
            "launch countdown started at {} seconds for {} players",
            seconds_remaining,
            roster.len()
        ),
        LobbyEvent::LaunchCountdownTick { seconds_remaining } => {
            format!("launch countdown tick: {seconds_remaining}s remaining")
        }
        LobbyEvent::MatchLaunchReady { roster } => {
            format!("match launch ready with roster size {}", roster.len())
        }
        LobbyEvent::MatchAborted { message, .. } => format!("match aborted: {message}"),
    }
}

fn render_match_event(event: &MatchEvent) -> String {
    match event {
        MatchEvent::SkillChosen { player_id, choice } => format!(
            "player {} locked {:?} tier {}",
            player_id.get(),
            choice.tree,
            choice.tier
        ),
        MatchEvent::PreCombatStarted { seconds_remaining } => {
            format!("pre-combat started with {seconds_remaining}s remaining")
        }
        MatchEvent::CombatStarted => String::from("combat started"),
        MatchEvent::RoundWon {
            round,
            winning_team,
            score,
        } => format!(
            "round {} won by {} (score {}-{})",
            round.get(),
            winning_team,
            score.team_a,
            score.team_b
        ),
        MatchEvent::MatchEnded {
            outcome,
            message,
            score,
        } => format!(
            "match ended as {:?}: {} (score {}-{})",
            outcome, message, score.team_a, score.team_b
        ),
        MatchEvent::ManualResolutionRequired { reason } => {
            format!("manual resolution required: {reason}")
        }
    }
}

fn render_sim_event(event: &SimulationEvent) -> String {
    match event {
        SimulationEvent::PlayerMoved { player_id, x, y } => {
            format!("player {} moved to ({x}, {y})", player_id.get())
        }
        SimulationEvent::EffectSpawned { effect } => format!(
            "player {} spawned {:?} from ({}, {}) to ({}, {}) with radius {}",
            effect.owner.get(),
            effect.kind,
            effect.x,
            effect.y,
            effect.target_x,
            effect.target_y,
            effect.radius
        ),
        SimulationEvent::DamageApplied {
            attacker,
            target,
            amount,
            remaining_hit_points,
            defeated,
        } => format!(
            "player {} hit player {} for {} (remaining hp {}, defeated: {})",
            attacker.get(),
            target.get(),
            amount,
            remaining_hit_points,
            defeated
        ),
    }
}

fn default_record_store_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("var")
        .join("player_records.tsv")
}

fn default_web_client_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("static")
        .join("webclient")
}

fn parse_tick_interval(raw: Option<String>) -> Duration {
    raw.and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map_or_else(|| Duration::from_secs(1), Duration::from_millis)
}

fn parse_log_format_from_env() -> Result<LogFormat, String> {
    env::var("RARENA_LOG_FORMAT")
        .map_or_else(|_| Ok(LogFormat::Pretty), |raw| LogFormat::parse(&raw))
}

fn init_tracing(log_format: LogFormat) -> Result<(), String> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info,axum=info,tower_http=info"))
        .map_err(|error| format!("failed to configure tracing filter: {error}"))?;

    match log_format {
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().compact())
            .try_init()
            .map_err(|error| format!("failed to initialize tracing: {error}")),
        LogFormat::Json => tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|error| format!("failed to initialize tracing: {error}")),
    }
}

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

#[allow(clippy::too_many_lines)]
fn run_demo() -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let alice_id = player_id(1);
    let bob_id = player_id(2);

    let mut lobby = Lobby::new(LobbyId::new(1).map_err(|error| error.to_string())?);
    lines.push(render_lobby_event(
        &lobby
            .add_player(alice_id, player_name("Alice"), PlayerRecord::new())
            .map_err(|error| error.to_string())?,
    ));
    lines.push(render_lobby_event(
        &lobby
            .add_player(bob_id, player_name("Bob"), PlayerRecord::new())
            .map_err(|error| error.to_string())?,
    ));

    for event in lobby
        .select_team(alice_id, TeamSide::TeamA)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_lobby_event(&event));
    }
    for event in lobby
        .select_team(bob_id, TeamSide::TeamB)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_lobby_event(&event));
    }
    for event in lobby
        .set_ready(alice_id, ReadyState::Ready)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_lobby_event(&event));
    }
    for event in lobby
        .set_ready(bob_id, ReadyState::Ready)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_lobby_event(&event));
    }

    let mut roster = None;
    for _ in 0..5 {
        let event = lobby
            .advance_countdown()
            .map_err(|error| error.to_string())?;
        if let LobbyEvent::MatchLaunchReady {
            roster: launch_roster,
        } = &event
        {
            roster = Some(launch_roster.clone());
        }
        lines.push(render_lobby_event(&event));
    }

    let roster = roster.ok_or_else(|| String::from("demo never produced a launch roster"))?;
    let mut session = MatchSession::new(
        MatchId::new(1).map_err(|error| error.to_string())?,
        roster.clone(),
        MatchConfig::v1(),
    )
    .map_err(|error| error.to_string())?;

    for event in session
        .submit_skill_pick(alice_id, skill(SkillTree::Mage, 1))
        .map_err(|error| error.to_string())?
    {
        lines.push(render_match_event(&event));
    }
    for event in session
        .submit_skill_pick(bob_id, skill(SkillTree::Rogue, 1))
        .map_err(|error| error.to_string())?
    {
        lines.push(render_match_event(&event));
    }
    for event in session
        .advance_phase_by(5)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_match_event(&event));
    }

    let mut world = SimulationWorld::new(vec![
        SimPlayerSeed {
            assignment: roster[0].clone(),
            hit_points: 100,
        },
        SimPlayerSeed {
            assignment: roster[1].clone(),
            hit_points: 100,
        },
    ])
    .map_err(|error| error.to_string())?;

    world
        .submit_input(
            alice_id,
            MovementIntent::new(1, 0).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    for event in world.tick() {
        lines.push(render_sim_event(&event));
    }

    for event in world
        .cast_skill(alice_id, 1)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_sim_event(&event));
    }

    for event in session
        .mark_player_defeated(bob_id)
        .map_err(|error| error.to_string())?
    {
        lines.push(render_match_event(&event));
    }

    let disconnect_event = session
        .disconnect_player(alice_id)
        .map_err(|error| error.to_string())?;
    lines.push(render_match_event(&disconnect_event));

    Ok(lines)
}

#[tokio::main]
async fn main() {
    let mode = env::args().nth(1);
    if matches!(mode.as_deref(), Some("--demo")) {
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

    let bind_address = env::var("RARENA_BIND").unwrap_or_else(|_| String::from("127.0.0.1:3000"));
    let listener = match tokio::net::TcpListener::bind(&bind_address).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(bind_address, %error, "dedicated_server failed to bind");
            std::process::exit(1);
        }
    };

    let record_store_path = env::var_os("RARENA_RECORD_STORE_PATH")
        .map_or_else(default_record_store_path, PathBuf::from);
    let web_client_root =
        env::var_os("RARENA_WEB_CLIENT_ROOT").map_or_else(default_web_client_root, PathBuf::from);
    let tick_interval = parse_tick_interval(env::var("RARENA_TICK_INTERVAL_MS").ok());

    let server = match spawn_dev_server_with_options(
        listener,
        DevServerOptions {
            tick_interval,
            record_store_path,
            web_client_root,
            observability: DevServerOptions::default().observability,
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
        websocket_url = %format!("ws://{}/ws", server.local_addr()),
        "dedicated_server listening"
    );
    wait_for_shutdown_signal().await;
    info!("shutdown signal received, stopping dedicated_server");
    server.shutdown().await;
    info!("dedicated_server stopped");
}

#[cfg(test)]
mod tests {
    use super::{parse_tick_interval, run_demo, LogFormat};
    use std::time::Duration;

    #[test]
    fn demo_script_produces_the_expected_vertical_slice_markers() {
        let output = run_demo().expect("demo should run");
        let joined = output.join("\n");

        assert!(joined.contains("launch countdown started"));
        assert!(joined.contains("combat started"));
        assert!(joined.contains("round 1 won by Team A"));
        assert!(joined.contains("NoContest"));
    }

    #[test]
    fn parse_tick_interval_uses_default_for_missing_zero_or_invalid_values() {
        assert_eq!(parse_tick_interval(None), Duration::from_secs(1));
        assert_eq!(
            parse_tick_interval(Some(String::from("0"))),
            Duration::from_secs(1)
        );
        assert_eq!(
            parse_tick_interval(Some(String::from("abc"))),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn parse_tick_interval_accepts_positive_milliseconds() {
        assert_eq!(
            parse_tick_interval(Some(String::from("25"))),
            Duration::from_millis(25)
        );
    }

    #[test]
    fn parse_log_format_from_env_accepts_pretty_and_json_and_rejects_unknown_values() {
        assert_eq!(
            LogFormat::parse("json").expect("json log format should parse"),
            LogFormat::Json
        );
        assert_eq!(
            LogFormat::parse("PRETTY").expect("pretty log format should parse"),
            LogFormat::Pretty
        );
        assert_eq!(
            LogFormat::parse("xml").expect_err("unknown log formats should be rejected"),
            "unsupported RARENA_LOG_FORMAT 'xml'; expected 'pretty' or 'json'"
        );
    }
}
