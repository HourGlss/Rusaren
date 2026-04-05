use game_content::GameContent;
use game_domain::{
    LobbyId, MatchId, PlayerId, PlayerName, PlayerRecord, ReadyState, SkillChoice, SkillTree,
    TeamSide,
};
use game_lobby::{Lobby, LobbyEvent};
use game_match::{MatchConfig, MatchSession};
use game_sim::{MovementIntent, SimPlayerSeed, SimulationWorld, COMBAT_FRAME_MS};

use crate::render::{render_lobby_event, render_match_event, render_sim_event};

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

#[allow(clippy::too_many_lines)]
pub(crate) fn run_demo() -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let alice_id = player_id(1);
    let bob_id = player_id(2);
    let content = GameContent::bundled().map_err(|error| error.to_string())?;

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
        MatchConfig::v1(content.map().objective_target_ms),
    )
    .map_err(|error| error.to_string())?;

    let alice_choice = skill(SkillTree::Mage, 1);
    let bob_choice = skill(SkillTree::Rogue, 1);

    for event in session
        .submit_skill_pick(alice_id, alice_choice.clone())
        .map_err(|error| error.to_string())?
    {
        lines.push(render_match_event(&event));
    }
    for event in session
        .submit_skill_pick(bob_id, bob_choice.clone())
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

    let mut world = SimulationWorld::new(
        vec![
            SimPlayerSeed {
                assignment: roster[0].clone(),
                hit_points: 100,
                melee: content
                    .skills()
                    .melee_for(&alice_choice.tree)
                    .ok_or_else(|| String::from("demo melee content should exist"))?
                    .clone(),
                skills: [
                    content.skills().resolve(&alice_choice).cloned(),
                    None,
                    None,
                    None,
                    None,
                ],
            },
            SimPlayerSeed {
                assignment: roster[1].clone(),
                hit_points: 100,
                melee: content
                    .skills()
                    .melee_for(&bob_choice.tree)
                    .ok_or_else(|| String::from("demo melee content should exist"))?
                    .clone(),
                skills: [
                    content.skills().resolve(&bob_choice).cloned(),
                    None,
                    None,
                    None,
                    None,
                ],
            },
        ],
        content.map(),
    )
    .map_err(|error| error.to_string())?;

    world
        .submit_input(
            alice_id,
            MovementIntent::new(1, 0).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    for event in world.tick(COMBAT_FRAME_MS) {
        lines.push(render_sim_event(&event));
    }

    world
        .queue_cast(alice_id, 1)
        .map_err(|error| error.to_string())?;
    for _ in 0..6 {
        for event in world.tick(COMBAT_FRAME_MS) {
            lines.push(render_sim_event(&event));
        }
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
