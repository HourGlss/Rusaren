//! Dedicated server entrypoint.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::expect_used))]

use std::env;

use game_api::spawn_dev_server;
use game_domain::{
    LobbyId, MatchId, PlayerId, PlayerName, PlayerRecord, ReadyState, SkillChoice, SkillTree,
    TeamSide,
};
use game_lobby::{Lobby, LobbyEvent};
use game_match::{MatchConfig, MatchEvent, MatchSession};
use game_sim::{MovementIntent, SimPlayerSeed, SimulationEvent, SimulationWorld};

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

    let damage_event = world
        .apply_damage(alice_id, bob_id, 100)
        .map_err(|error| error.to_string())?;
    lines.push(render_sim_event(&damage_event));

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

    let bind_address = env::var("RARENA_BIND").unwrap_or_else(|_| String::from("127.0.0.1:3000"));
    let listener = match tokio::net::TcpListener::bind(&bind_address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("dedicated_server failed to bind {bind_address}: {error}");
            std::process::exit(1);
        }
    };

    let server = match spawn_dev_server(listener).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("dedicated_server failed to start websocket adapter: {error}");
            std::process::exit(1);
        }
    };

    println!(
        "dedicated_server websocket adapter listening on ws://{}/ws",
        server.local_addr()
    );
    std::future::pending::<()>().await;
}

#[cfg(test)]
mod tests {
    use super::run_demo;

    #[test]
    fn demo_script_produces_the_expected_vertical_slice_markers() {
        let output = run_demo().expect("demo should run");
        let joined = output.join("\n");

        assert!(joined.contains("launch countdown started"));
        assert!(joined.contains("combat started"));
        assert!(joined.contains("round 1 won by Team A"));
        assert!(joined.contains("NoContest"));
    }
}
